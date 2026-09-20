use std::{borrow::Cow, error::Error, sync::Arc};

use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig, Tool,
        ToolAnnotations,
    },
    service::RequestContext,
};
use rusqlite::{Connection, params};
use serde_json::{Map, Value, json};
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

const MODERN: ProtocolVersion = ProtocolVersion::V_2026_07_28;
const LEGACY: ProtocolVersion = ProtocolVersion::V_2025_11_25;
const SUPPORTED_PROTOCOLS: &[ProtocolVersion] = &[LEGACY, MODERN];

struct GrammarCase {
    target: &'static str,
    language: Language,
    fixture: &'static str,
    query: &'static str,
    expected_capture: &'static str,
    strict: bool,
}

fn grammar_cases() -> Vec<GrammarCase> {
    vec![
        GrammarCase {
            target: "Dart",
            language: tree_sitter_dart::LANGUAGE.into(),
            fixture: include_str!("../fixtures/basic.dart"),
            query: include_str!("../queries/dart.scm"),
            expected_capture: "DartBasic",
            strict: true,
        },
        GrammarCase {
            target: "Dart-modern",
            language: tree_sitter_dart::LANGUAGE.into(),
            fixture: include_str!("../fixtures/modern.dart"),
            query: include_str!("../queries/dart.scm"),
            expected_capture: "UserId",
            strict: false,
        },
        GrammarCase {
            target: "C#",
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            fixture: include_str!("../fixtures/basic.cs"),
            query: include_str!("../queries/c_sharp.scm"),
            expected_capture: "CSharpBasic",
            strict: true,
        },
        GrammarCase {
            target: "Rust",
            language: tree_sitter_rust::LANGUAGE.into(),
            fixture: include_str!("../fixtures/basic.rs"),
            query: include_str!("../queries/rust.scm"),
            expected_capture: "rust_basic",
            strict: true,
        },
        GrammarCase {
            target: "Go",
            language: tree_sitter_go::LANGUAGE.into(),
            fixture: include_str!("../fixtures/basic.go"),
            query: include_str!("../queries/go.scm"),
            expected_capture: "goBasic",
            strict: true,
        },
        GrammarCase {
            target: "Java",
            language: tree_sitter_java::LANGUAGE.into(),
            fixture: include_str!("../fixtures/Basic.java"),
            query: include_str!("../queries/java.scm"),
            expected_capture: "JavaBasic",
            strict: true,
        },
        GrammarCase {
            target: "JavaScript",
            language: tree_sitter_javascript::LANGUAGE.into(),
            fixture: include_str!("../fixtures/basic.js"),
            query: include_str!("../queries/javascript.scm"),
            expected_capture: "javascriptBasic",
            strict: true,
        },
        GrammarCase {
            target: "JSX",
            language: tree_sitter_javascript::LANGUAGE.into(),
            fixture: include_str!("../fixtures/basic.jsx"),
            query: include_str!("../queries/jsx.scm"),
            expected_capture: "CompatibilityCard",
            strict: true,
        },
        GrammarCase {
            target: "TypeScript",
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            fixture: include_str!("../fixtures/basic.ts"),
            query: include_str!("../queries/typescript.scm"),
            expected_capture: "TypeScriptBasic",
            strict: true,
        },
        GrammarCase {
            target: "TSX",
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            fixture: include_str!("../fixtures/basic.tsx"),
            query: include_str!("../queries/tsx.scm"),
            expected_capture: "TsxCard",
            strict: true,
        },
    ]
}

fn run_grammar_probe() -> Result<(), Box<dyn Error>> {
    println!(
        "tree-sitter runtime ABI={}..={}",
        tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION,
        tree_sitter::LANGUAGE_VERSION
    );

    for case in grammar_cases() {
        let mut parser = Parser::new();
        parser.set_language(&case.language)?;
        let tree = parser
            .parse(case.fixture, None)
            .ok_or_else(|| format!("{} parser cancelled", case.target))?;
        let root = tree.root_node();
        let query = Query::new(&case.language, case.query)?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, case.fixture.as_bytes());
        let mut captures = Vec::new();
        while let Some(query_match) = matches.next() {
            for capture in query_match.captures() {
                captures.push(capture.node.utf8_text(case.fixture.as_bytes())?.to_owned());
            }
        }
        let expected_found = captures
            .iter()
            .any(|capture| capture == case.expected_capture);

        println!(
            "grammar target={} abi={} parse_has_error={} query_captures={} expected_capture={}",
            case.target,
            case.language.abi_version(),
            root.has_error(),
            captures.len(),
            expected_found
        );
        if case.strict && root.has_error() {
            return Err(format!("{} fixture contains ERROR or MISSING nodes", case.target).into());
        }
        if !expected_found {
            return Err(format!(
                "{} query did not capture {}",
                case.target, case.expected_capture
            )
            .into());
        }
    }
    Ok(())
}

fn parse_version(version: &str) -> Result<(u32, u32, u32), Box<dyn Error>> {
    let mut components = version.split('.');
    let major = components.next().ok_or("missing SQLite major")?.parse()?;
    let minor = components.next().ok_or("missing SQLite minor")?.parse()?;
    let patch = components.next().ok_or("missing SQLite patch")?.parse()?;
    Ok((major, minor, patch))
}

fn run_sqlite_probe() -> Result<(), Box<dyn Error>> {
    let mut connection = Connection::open_in_memory()?;
    let version: String = connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    if parse_version(&version)? < (3, 51, 3) {
        return Err(format!("bundled SQLite {version} is below policy floor 3.51.3").into());
    }
    let fts5: i64 = connection.query_row(
        "SELECT sqlite_compileoption_used('ENABLE_FTS5')",
        [],
        |row| row.get(0),
    )?;
    if fts5 != 1 {
        return Err("bundled SQLite does not report ENABLE_FTS5".into());
    }

    connection.execute("CREATE VIRTUAL TABLE documents USING fts5(body)", [])?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO documents(body) VALUES (?1)",
        params!["CodeAtlas compatibility evidence"],
    )?;
    let count: i64 = transaction.query_row(
        "SELECT count(*) FROM documents WHERE documents MATCH 'compatibility'",
        [],
        |row| row.get(0),
    )?;
    if count != 1 {
        return Err(format!("FTS5 search returned {count} rows, expected 1").into());
    }
    transaction.execute("DELETE FROM documents", [])?;
    let remaining: i64 =
        transaction.query_row("SELECT count(*) FROM documents", [], |row| row.get(0))?;
    if remaining != 0 {
        return Err("FTS5 delete did not remove the inserted row".into());
    }
    transaction.commit()?;

    let mut statement = connection.prepare("PRAGMA compile_options")?;
    let options = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    println!(
        "sqlite version={version} fts5=true transaction=pass compile_options={}",
        options.join(",")
    );
    Ok(())
}

#[derive(Clone, Debug)]
struct StatusServer;

impl StatusServer {
    fn status_tool() -> Tool {
        let input_schema = Map::from_iter([
            ("type".to_owned(), Value::String("object".to_owned())),
            ("properties".to_owned(), Value::Object(Map::new())),
            ("additionalProperties".to_owned(), Value::Bool(false)),
        ]);
        Tool::new(
            "compatibility_status",
            "Return the disposable Phase 00 status payload",
            Arc::new(input_schema),
        )
        .with_annotations(
            ToolAnnotations::new()
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
        )
    }
}

impl ServerHandler for StatusServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "codeatlas-compatibility",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("Disposable Phase 00 compatibility status server")
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_PROTOCOLS)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(vec![
            Self::status_tool(),
        ])))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        (name == "compatibility_status").then(Self::status_tool)
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + '_ {
        let result = if request.name == "compatibility_status" {
            CallToolResult::structured(json!({
                "status": "ready",
                "protocols": ["2026-07-28", "2025-11-25"]
            }))
        } else {
            CallToolResult::structured_error(json!({
                "status": "error",
                "message": "unknown compatibility tool"
            }))
        };
        std::future::ready(Ok(result.into()))
    }
}

async fn serve() -> Result<(), Box<dyn Error>> {
    let running = StatusServer.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

fn probe() -> Result<(), Box<dyn Error>> {
    println!(
        "host={} arch={} rustc={} rmcp=3.4.0 protocols=2026-07-28,2025-11-25",
        std::env::consts::OS,
        std::env::consts::ARCH,
        option_env!("RUSTC_VERSION").unwrap_or("captured-by-command")
    );
    run_grammar_probe()?;
    run_sqlite_probe()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("serve") => serve().await,
        Some("probe") | None => probe(),
        Some(command) => Err(format!("unknown command: {command}; use probe or serve").into()),
    }
}
