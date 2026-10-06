# CodeAtlas

Codex ve diğer MCP istemcileri için yerel öncelikli kod analizi.

[English](README.md) · [Komut referansı](docs/COMMAND_REFERENCE.md) ·
[Güvenlik ve gizlilik](docs/SECURITY_PRIVACY.md)

CodeAtlas, bir kaynak kod deposunu indeksleyerek kodlama ajanına sembol arama,
referans ve çağrı noktalarını inceleme, ilişkileri keşfetme ve sınırlı kaynak kodu
bağlamı toplama araçları sunar. Rust ile geliştirilmiştir; Tree-sitter ve kalıcı
SQLite/FTS5 indeksi kullanır. Kendi içinde yapay zekâ modeli çalıştırmaz.

Şu an doğrulanmış platform **macOS ARM64 (Apple Silicon)**. Linux, Windows ve Intel
Mac desteği henüz doğrulanmadı. Yayımlanmış bir binary release yoktur; yerel paketler
imzasızdır ve notarize edilmemiştir. Native testler macOS 27.0 üzerinde çalıştırıldı.

## Ne amaçla kullanılır?

Bir kodlama ajanının projeyi anlaması, hata araştırması, kod açıklaması veya değişiklik
planlaması gerektiğinde kullanılır. Her istekte bütün depoyu modele vermek yerine,
ilgili kanıtı dosya konumu, içerik hash'i ve belirsizlik bilgisiyle getirir.

- Fonksiyon, sınıf, metot, alan ve diğer deklarasyonları ada göre bulur.
- Dosya taslağı, referans, gözlenen çağrı ve sınırlı bağımlılık grafiği sunar.
- Olası değişiklik etkisini incelemeye ve boyutu sınırlı kod bağlamı toplamaya yardımcı olur.
- Kalıcı indeksi tekrar kullanır; artımlı güncelleme ve isteğe bağlı dosya izleme sağlar.
- Açıkça yazılmış proje notlarını okur; yazma yalnız güvenilir başlangıç seçeneğiyle açılır.

İstemcinin başlattığı tek bir **stdio MCP süreci** olarak çalışır. Zorunlu daemon,
ağ portu, Docker, Redis, Neo4j, embedding servisi, API anahtarı veya model indirmesi
gerekmez. Paketlenmiş uygulama kaynak analizi için Dart, .NET, Go, Java veya Node
SDK'sına ihtiyaç duymaz. MCP istemcisinin hesap/model gereksinimleri ayrıdır.

CodeAtlas bir derleyici veya language server değildir. Tree-sitter sözdizimi kanıtı
sağlar; `syntax_observation`, `lexically_resolved`, `candidate` ve `unresolved`
kesinliği ayırır. Dinamik dispatch, macro expansion, overload seçimi veya desteklenmeyen
modül eşlemeleri kesin çağrı gibi gösterilmez. Grafikte ilişki yoksa bağımlılık yok
sonucu çıkarılmamalıdır.

## Desteklenen programlama dilleri

| Dil | İndekslenen uzantılar | Kapsam notu |
|---|---|---|
| Dart | `.dart` | Test edilmiş modern Dart ve Flutter benzeri sözdizimi; analyzer/Flutter SDK entegrasyonu yok |
| C# | `.cs` | Namespace, tip, metot ve partial deklarasyon gözlemleri; .NET derleyici çözümlemesi yok |
| Rust | `.rs` | Modül, trait, impl ve `use` alias'ları; macro expansion ve aktif `cfg` seçimi yok |
| Go | `.go` | Paket, import ve receiver metotları; paket bağlama dizin sınırlarını korur |
| Java | `.java` | Paket, tip, metot ve import; classpath veya çalışma zamanı dispatch çözümlemesi yok |
| JavaScript / JSX | `.js`, `.mjs`, `.cjs`, `.jsx` | ESM ve tanınan CommonJS; JSX etiketleri referanstır, otomatik çağrı ilişkisi değildir |
| TypeScript / TSX | `.ts`, `.mts`, `.cts`, `.tsx` | Deklarasyon dosyaları, tip/değer rolleri ve ayrı TSX grammar'ı; TypeScript type checker yok |

Yedi dilin ve ayrı JSX/TSX sağlayıcılarının parsing, çıkarım ve negatif fixture'ları
vardır; bu eksiksiz ekosistem desteği değildir. Sabitlenen Dart grammar'ı ASCII dışı
identifier'ları desteklemez. Vue/Svelte, Python, PHP, C/C++, Kotlin ve notebook
kapsam dışıdır. Ayrıntılar: [dil sözleşmesi](docs/LANGUAGE_SUPPORT.md) ve
[grammar matrisi](docs/GRAMMAR_MATRIX.md).

## macOS Apple Silicon kurulumu

### Kaynaktan derleme

Gereksinimler: Git, `rustup` üzerinden kurulmuş Rust ve Xcode Command Line Tools veya
uygun Xcode C/C++ toolchain'i. Depo `rust-toolchain.toml` içinde Rust **1.98.1**
sürümünü sabitler. Derleme bağımlılık indirmek için ağ kullanabilir; varsayılan
çalışma zamanı ağ kullanmaz.

```sh
git clone https://github.com/devtosun/code-atlas.git
cd code-atlas
cargo build --release --locked -p ca-cli
./target/release/codeatlas --version
```

Varsayılan Cargo çıktı dizininde binary `target/release/codeatlas` olur.
`CARGO_TARGET_DIR` ayarladıysan onun altındaki `release/codeatlas` yolunu kullan.
Uygulamayı MCP istemcisi için sabit, mutlak bir yolda tut. Global PATH değişikliği
veya yönetici yetkisiyle kurulum gerekmez; Python çalışma zamanı bağımlılığı değildir.

### Yerel paket alternatifi

Native macOS ARM64 derleme makinesinde, yalnız paketleme için Python 3.11+ gerekir:

```sh
cargo fetch --locked
python3 scripts/package_macos.py
python3 scripts/phase15_package_smoke.py \
  dist/codeatlas-0.1.0-aarch64-apple-darwin.tar.gz
```

Paket; binary, hızlı başlangıç, gizlilik rehberi, bağımlılık bildirimleri ve checksum
içerir. Arşivi ve `.sha256` dosyasını güvenilir üreticiden edin; varsayılabilecek
yayımlanmış bir indirme adresi yoktur. İki dosyanın bulunduğu dizinde:

```sh
shasum -a 256 -c codeatlas-0.1.0-aarch64-apple-darwin.tar.gz.sha256
tar -xzf codeatlas-0.1.0-aarch64-apple-darwin.tar.gz
cd codeatlas-0.1.0-aarch64-apple-darwin
shasum -a 256 -c CHECKSUMS.sha256
./bin/codeatlas --version
```

Mevcut kurulumun üzerine değil, yeni bir dizine aç. İmzasız paket için macOS güvenlik
kontrollerini körlemesine devre dışı bırakma.

## İlk indeksin oluşturulması

İki yolu binary ve **analiz edeceğin proje** ile değiştir. Analiz edilecek proje
CodeAtlas'ın kendi kaynak deposu olmak zorunda değildir. Yollar mutlak olmalıdır.

```sh
CODEATLAS_BIN="/absolute/path/to/code-atlas/target/release/codeatlas"
CODEATLAS_PROJECT="/absolute/path/to/your/project"

"$CODEATLAS_BIN" doctor --root "$CODEATLAS_PROJECT" --json
"$CODEATLAS_BIN" index --root "$CODEATLAS_PROJECT" --json
"$CODEATLAS_BIN" status --root "$CODEATLAS_PROJECT" --json
```

`index` değişmeyen dosya sürümlerini tekrar kullanır. Düzenlemelerden sonra yeniden
çalıştır; tüm desteklenen kaynakları parse etmek için `index --full` kullan.
Başarısız/iptal edilmiş nesil sağlıklı aktif indeksi değiştirmez. Şema v7'ye yükseltince
düzeltilmiş analizler için açıkça yeniden indeksle; eski binary'ye dönmeden önce
tutarlı bir veritabanı yedeği al.

Worktree'ye özel veritabanı kaynak deponun dışında,
`~/Library/Application Support/CodeAtlas/worktrees/<repository-id>/` altında tutulur.
`doctor` gerçek yolu ve writer/follower rolünü gösterir. Notlar yeniden indekslemede korunur.

## Codex'e ekleme

Codex, stdio sunucularını TOML ayarlarındaki `[mcp_servers.<ad>]` tablosuyla yapılandırır.
Bkz. [resmi MCP belgeleri](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).
CodeAtlas'ın yardımcısı diğer ayarları ve MCP girdilerini korur.

### Önerilen yöntem: önce önizle, sonra uygula

Önceki bölümdeki mutlak yollarla, yalnız ilgili girdi için farkı önizle:

```sh
"$CODEATLAS_BIN" integrate codex --root "$CODEATLAS_PROJECT" --dry-run
```

Farkı incele, ardından açıkça uygula:

```sh
"$CODEATLAS_BIN" integrate codex --root "$CODEATLAS_PROJECT" --apply
```

`CODEX_HOME` ayarlıysa `$CODEX_HOME/config.toml`, değilse `~/.codex/config.toml`
kullanılır. Başka dosya seçmek için `--config /absolute/path/to/config.toml` ekle.
Yardımcı mevcut config'i yedekler, mutlak binary yolu yazar, `required = false`
ayarlar ve bozuk TOML veya kendisine ait olmayan ad çakışmasını reddeder.
PATH'i, diğer sunucuları veya global execution ayarlarını değiştirmez.

### Alternatif: elle yapılandırma

Config'i yedekle ve **yalnız bu tabloyu** birleştir; iki yolu değiştir. Dosyanın
tamamını değiştirme ve ikinci bir `codeatlas` tablosu ekleme.

```toml
[mcp_servers.codeatlas]
command = "/absolute/path/to/code-atlas/target/release/codeatlas"
args = ["serve", "--root", "/absolute/path/to/your/project"]
required = false
startup_timeout_sec = 10
tool_timeout_sec = 60
```

Ayarları almak için istemciyi/oturumu yeniden başlat. Codex CLI kullanıyorsan
`codex mcp list` ile yapılandırılmış sunucuları kontrol et. `serve` komutunu istemci
başlatır; etkileşimli arama terminali değildir ve stdout yalnız MCP trafiğine ayrılır.

Örnek istekler:

- “CodeAtlas ile bu fonksiyonu bul ve referanslarını incele.”
- “Bu modülün gözlenen bağımlılıklarını çıkar; belirsiz ilişkileri ayrıca belirt.”
- “Değişiklik önermeden önce ilgili koddan sınırlı bir bağlam oluştur.”

Varsayılan **14 araç** arasında `index_repository`, `search_symbols`, `get_symbol`,
`find_references`, `trace_calls`, `read_code`, `analyze_impact`, `build_context` ve
`search_memories` bulunur. İndeksleme açık bir çağrıyla başlatılır, handshake'in parçası değildir.
MCP indeksleme çağrısı iş kimliği döndürür; `job_status` ile takip, `cancel_job`
ile iptal edilir. Bkz. [tüm araç sözleşmeleri](docs/MCP_CONTRACT.md).

### İsteğe bağlı izleme ve not yazma

Writer-owner çalışırken mevcut indeksi güncel tutmak için `serve` argümanlarına
`--watch` ekle. İzleme açıkken `--watch-poll` polling seçer. Olaylar ipucudur;
periyodik uzlaştırma ayrıca içerik hash'lerini kontrol eder.

Yalnız istemcinin proje notu yazmasına güveniyorsan `--memory-write` ekle.
`upsert_memory` ve `forget_memory` açılır (toplam 16 araç); bu kaynak düzenleme
izni değildir. İzleme ve not yazma varsayılan olarak kapalıdır.

Her worktree veritabanı için tek yazıcı olabilir. Follower istemciler aktif indeksi
okur; yazma işlemi tekrar denenebilir `WRITER_BUSY` alır. Codex zaten yazıcıysa
ikinci CLI yazıcısı açmak yerine indekslemeyi o istemciden iste.

## Güvenlik ve kaldırma

- Yalnız yetkili kökler okunur. `.gitignore`, `.ignore`, `.codeatlasignore` uygulanır;
  policy symlink kaçışlarını, credential ve üretilmiş/derleme yollarını dışlar.
- İndekslenen kod, build script, hook, package manager veya depo talimatları
  çalıştırılmaz. Kaynak ve notlar güvenilmeyen veridir.
- SQLite şifrelenmemiştir. **Yerel indeks uçtan uca yerel AI değildir:** uzak
  kodlama ajanı getirilen kaynak ve notları alabilir.
- Gönderilen MCP frame'leri toplam 65.536 serialize byte ile sınırlıdır; ID ve
  girdilerin ayrıca sınırları vardır. Bu doğruluk veya injection bağışıklığı garantisi değildir.

Yardımcının oluşturduğu girdiyi kaldırmak için önce önizle:

```sh
"$CODEATLAS_BIN" integrate codex --remove --dry-run
"$CODEATLAS_BIN" integrate codex --remove --apply
```

Kurulumda `--config` kullandıysan aynı yolu ekle. Yalnız yardımcının kendi girdisi
kaldırılır; kaynak, indeks, not ve yedek silinmez. Elle eklenmiş sahipsiz tabloyu
inceleyerek elle kaldır; yardımcı o tabloyu sahiplenmez.

## Geliştirme ve doğrulama

Proje aşamalı geliştirme kiti olarak başladı; artık çalışan bir sunucu içeriyor.
Son düzeltmelerde 130 workspace testi, format, Clippy, bütün mevcut dil/korpus
kapıları ve çıkarılmış macOS paket testleri geçti. Ayrıntılar ve çalıştırılmayan kontroller:

- [Proje durumu](docs/PROJECT_STATE.md) ve [test matrisi](docs/TEST_MATRIX.md)
- [İnceleme düzeltmeleri ve yükseltme notları](docs/reports/review-fixes.md)
- [macOS paketleme raporu](docs/reports/15-release-and-codex.md)
- [Mimari](docs/ARCHITECTURE.md) ve [bağımlılık politikası](docs/DEPENDENCY_POLICY.md)
- [Geliştirme başlangıcı](BOOTSTRAP_PROMPT.tr.md) ve [aşama manifesti](config/phase-manifest.json)
- [Ertelenen Linux/Windows doğrulaması](prompts/14-linux-windows-native-validation.md)
- [Lisans bildirimi](LICENSE-NOTICE.md)

Temel katkıcı kontrolleri:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/validate_kit.py
```

Güncel düzeltmeler için yeni fuzz/advisory taraması veya model-backed Codex oturumu
çalıştırılmadı; eski sonuçlar tarihsel kanıttır. İmzalama, notarization, public binary
release ve Linux/Windows doğrulaması ayrı çalışmalardır.
