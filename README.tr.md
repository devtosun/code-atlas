# CodeAtlas MCP — Rust ile geliştirme rehberi

**Paket türü:** Codex ile aşamalı uygulama geliştirme kiti ve engelleri açıkça
kaydedilmiş Aşama 14 Rust çalışma alanı.
**Araştırma tarihi:** 20 Eylül 2026. **Durum:** macOS ARM64 için Aşama 14 tamamlandı;
Aşama 15 sıradaki adımdır.

Şu anda `serve`, gerçek depolama durumunu kullanan `doctor`, kalıcı indeks oluşturan
`index` ve `status` komutları çalışır. MCP yüzeyi status, asenkron indeks işleri,
arama, grafik sorguları, güncellik doğrulamalı kaynak okuma ve bağlam oluşturmayı
kapsayan araçlara ek olarak açık proje notları sunar. Varsayılan yüzey on dört,
güvenilir `--memory-write` seçeneğiyle on altı tipli araç içerir. Sınırlı
lexical/import çözümleme, nesil-kapsamlı
kanıt grafiği ve yalnız yazıcı sahibinde açıkça etkinleştirilen, periyodik tarama/hash
uzlaştırmalı watcher uygulanmıştır. Revizyonlu bellek, tipli resources ve statik MCP
prompt şablonları tamamlanmıştır. macOS hardening ve 10k/100k ölçümleri çalıştırılmış,
lookup ve debounce sonrası tek-dosya düzenleme performans hedefleri onarımdan sonra
geçmiştir. Ayrı nightly fuzz koşuları, advisory/license policy taramaları, eksiksiz
deklarasyon etiketleri ve stripped release doğrulaması macOS kapılarını kapatmıştır.
Linux ve Windows daha sonraya ertelenmiştir ve henüz desteklenmez; bunların akışı
`prompts/14-linux-windows-native-validation.md` dosyasındadır. Aşama 15 paketleme ve
Codex entegrasyonu henüz çalıştırılmadığı için yayın hazır olduğu iddia edilmez. Güncel
kanıt için `docs/PROJECT_STATE.md` ve
`docs/reports/14-hardening-and-evaluation.md` dosyalarına bakın.

## 1. Geliştirilecek ürün

CodeAtlas, bir kaynak kod deposunu okuyup sembol, kapsam, import, referans ve çağrı
noktalarını yerel bir indekse kaydeden; bunları Codex'e MCP araçları üzerinden sunan
bir Rust uygulaması olarak tasarlanmıştır. Amaç her soruda bütün projeyi modele
vermek yerine, sınırlı boyutta ve kaynak konumu belli bir bağlam üretmektir.
“CodeAtlas” bu kitin çalışma adıdır; hazır veya aynı isimdeki bir ürünün kurulumu değildir.

Zorunlu diller Dart, C#, Rust, Go, Java, JavaScript ve TypeScript'tir. JavaScript için
JSX, TypeScript için TSX ayrıca test edilir. Tree-sitter desteği yalnızca bir grammar
paketini eklemek demek değildir: her dil için gerçek declaration/import/scope/call
çıkarımı, negatif örnekler ve hatalı kaynak testleri tamamlanmalıdır.

İlk sürümde ücretli AI API, embedding servisi, vektör veritabanı, Redis veya Neo4j yoktur.
Sunucu kendi başına model çalıştırmaz. Kalıcı veri tek yerel SQLite veritabanında tutulur.
Codex istemcisi MCP sonuçlarını uzak modele gönderebilir; indeksleyicinin yerel olması
kaynak kodunun tüm kullanım boyunca cihazdan hiç çıkmayacağı anlamına gelmez.

## 2. Seçilen teknoloji ve mimari

| Alan | Seçim |
|---|---|
| Dil ve çalışma alanı | Rust 1.98.1 Aşama 00 denemesinde sabitlendi; Cargo workspace Aşama 01'de kuruldu |
| MCP | Resmî Rust SDK `rmcp`; `stdio`; zorunlu daemon veya HTTP sunucusu yok |
| Eşzamanlılık | Tokio; sınırlandırılmış kuyruklar; parser worker'ları; tek yazıcı thread |
| Kod çözümleme | Tree-sitter; derleme zamanında eklenen dil adaptörleri ve `.scm` sorguları |
| Kalıcılık ve arama | `rusqlite`; bundled SQLite; FTS5; WAL; atomik indeks nesilleri |
| Dosya keşfi/izleme | `ignore`, `globset`, `notify`; içerik hash'i için `blake3` |
| CLI / ayarlar | `clap`, TOML, `toml_edit`; mevcut ayarları koruyan entegrasyon |
| Git bilgileri | `gix`; repo script veya hook çalıştırmadan metadata okuma |
| Şema/log/hata | `serde`, SDK uyumlu `schemars`, `tracing`, `thiserror` |
| Testler | `insta`, `proptest`, `tempfile`, `criterion`, `cargo-fuzz`; Rust `xtask` |

Tree-sitter, grammar, rmcp ve SQLite seçimleri Aşama 00'da gerçek sonuçlarla
doğrulanmış; `Cargo.lock`, grammar uyumluluk matrisi ve sürüm raporu oluşturulmuştur.
Sonraki aşamalara ait diğer hedef bağımlılıklar henüz doğrulanmış sayılmaz.
Parser ve SQLite native bağımlılıkları nedeniyle derleme ortamında uygun C/C++
araçları gerekir. Nihai uygulamanın hedefi kullanıcıda Dart/.NET/Node/JVM/Go SDK'ları
kurulu olmadan kaynak dosyalarını çözümleyebilmektir; bunu native release testleri kanıtlar.

```text
Yetkilendirilmiş proje kökü
  → Ignore kurallarına uygun, sınırlandırılmış dosya taraması
  → İçerik hash'i + Tree-sitter dil adaptörü
  → Semboller / kapsamlar / importlar / çağrı noktaları
  → İlişki çözümleme: kesinlik ve aday ayrımı
  → SQLite'a yeni indeks nesli hazırlama
  → Başarılıysa aktif nesli atomik değiştirme
  → Arama / referans / çağrı grafiği / sınırlı bağlam
  → MCP → Codex
```

`crates/ca-core`, `ca-languages`, `ca-storage`, `ca-engine`, `ca-mcp`, `ca-cli`
çalışma alanını oluşturur. Repository, parser, dil çıkarımı, SQLite, sınırlı
indeksleme/job, kanıta dayalı resolution/graph, search/context, açık MCP araçları ve
owner-only watcher katmanları uygulanmıştır; daha sonraki aşamalar plan durumundadır.

## 3. Başlangıç — Codex'te nasıl kullanılacak?

Arşivi yeni, boş bir proje klasörüne aç. Gizli `.agents` klasörünün de açıldığından
emin ol. Mevcut bir projeye aktarılacaksa `AGENTS.md`, `.gitignore` ve diğer dosyaları
körlemesine üzerine yazma; uygun değişiklikleri karşılaştırarak birleştir. İndekslenecek
asıl iş projesi ile bu MCP'nin geliştirildiği proje farklı dizinler olabilir.

Codex'i kitin proje kökünde aç. [BOOTSTRAP_PROMPT.tr.md](BOOTSTRAP_PROMPT.tr.md)
içeriğini ilk mesaj olarak kullan. Ortamdaki yetkileri proje dosyaları ve gerekli
bağımlılık çözümlemesiyle sınırla; sistem genelinde tam yetki gerekli değildir.
Resmî belgelerdeki sürüm/API bilgileri gerektiğinde doğrulanmalıdır.

Aşamalar sırayla uygulanır. Tek mesajla tüm projeyi bitirmesini isteme. Her aşamanın
somut çıktısı, kabul testleri, kapsam dışı işleri ve rapor dosyası vardır. Aşama sonunda
raporu incele; mevcut kodun testlerini görmeden yalnızca “tamamlandı” sözünü yeterli
sayma. Bağımsız inceleme için `prompts/REVIEW.md` kullan; bu prompt üretim kodunu
inceleme sırasında değiştirmemesini ister. Hataları `prompts/FIX_FAILURES.md` ile düzelt.

Bir sonraki aşama için örnek:

```text
$ca-phase-driver $ca-rust-architecture $ca-mcp-contract

AGENTS.md ve docs/PROJECT_STATE.md dosyalarını oku.
prompts/01-workspace-and-lifecycle.md dosyasındaki aşamayı uygula.
Önce Aşama 00 raporundaki kabul sonuçlarını mevcut dosyalarla doğrula.
Yalnızca bu aşamada çalış. Test sonucunu uydurma, kapsamı genişletme.
Bana Türkçe bir sonuç özeti ver ve sonraki aşamaya otomatik geçme.
```

Yeni oturumda tüm eski sohbeti taşımak yerine `prompts/RESUME.md` kullan. Kalıcı
ilerleme kaydı `docs/PROJECT_STATE.md`, dil kapsamı `docs/GRAMMAR_MATRIX.md`, test
sonuçları `docs/TEST_MATRIX.md` ve `docs/reports/` üzerinden tutulur.

## 4. Aşama planı

Aşağıdaki dosyaların her biri tam bir uygulama promptudur; yalnızca başlık listesi değildir.
Aşama 00–15 temel ürün kapsamıdır. Aşama 16 ve 17 bağımsız opsiyonel geliştirmelerdir.

| No | Yapılacak iş | Geçişte aranacak kanıt |
|---|---|---|
| 00 | Toolchain, rmcp, tüm grammarlarda uyumluluk denemesi | Aynı runtime ile 7 dil + JSX/TSX; modern/eski MCP; gerçek sürümler |
| 01 | Cargo workspace, CLI ve hızlı MCP başlangıcı | İndeks/DB kilidi beklemeden protokol ve status yanıtı |
| 02 | Güvenli kök erişimi, worktree kimliği, SQLite | Kök dışına çıkamama; tek yazıcı; nesil/transaction testleri |
| 03 | Parser registry, normalize fact ve capture sözleşmesi | Query compile, byte/line aralığı, timeout/reset, fixture altyapısı |
| 04 | Rust ve Go çıkarıcıları | Sembol, kapsam, import, çağrı noktası; trait/interface belirsizliği |
| 05 | JS, JSX, TS ve TSX çıkarıcıları | Uygun grammar seçimi; import/export, shadowing, JSX/TSX örnekleri |
| 06 | C# ve Java çıkarıcıları | Overload ayrımı, partial class, annotation/attribute, çağrı örnekleri |
| 07 | Dart çıkarıcısı | Constructor, getter/setter, prefix/part, Flutter benzeri ve modern Dart testleri |
| 08 | İndeksleme işleri ve atomik yayınlama | İptal/çökme eski indeksi bozmaz; kısmi tarama toplu silme üretmez |
| 09 | İlişki çözümleme ve grafik | Yerel kapsam doğru; belirsiz hedefler aday; döngü ve sınırlar |
| 10 | Arama, etki analizi, bağlam oluşturma | Literal güvenli FTS, hash doğrulama, cursor ve toplam yanıt bütçesi |
| 11 | MCP araçlarını engine'e bağlama | Gerçek tool davranışı, şemalar, hata ayrımı; sahte başarı yok |
| 12 | Dosya izleme ve artımlı güncelleme | Rename/delete/atomic save/kayıp event sonrasında tutarlı indeks |
| 13 | Kalıcı notlar, resources ve MCP prompts | Notlar rebuild'de korunur; eski kanıt bayrağı; açık yazma izni |
| 14 | Güvenlik, fuzz, hata sonrası toparlanma, ölçümler | Gerçek test kayıtları, platform matrisi, doğruluk ve performans verisi |
| 15 | Paketleme, doctor, güvenli Codex entegrasyonu | Temiz makine smoke test; yedekli ve yalnız kendi girdisine müdahale |
| 16 | Opsiyonel framework çıkarıcıları | Framework ilişkileri yalnız kanıtlanan sözdizimi düzeyinde |
| 17 | Opsiyonel LSP ile semantik zenginleştirme | Onaylı sidecar; belge sürümü uyumlu kanıt; temel indeks bağımsız |

Tam dosya eşlemesi `config/phase-manifest.json` içindedir.

## 5. Skill dosyaları ve AGENTS.md

Proje skill'leri `.agents/skills/<skill-adı>/SKILL.md` altındadır. Her skill için
`agents/openai.yaml` metadata dosyası da vardır. `$ca-phase-driver` gibi adlarla açıkça
çağrılabilir. Güncel keşif/kullanım davranışı için `docs/SOURCES.md` içindeki resmî Codex
skill kaynağına bak. Skill'in kurulu olması testlerin yapıldığı anlamına gelmez.

| Skill | Görevi |
|---|---|
| ca-phase-driver | Tek aşama disiplini, önkoşul kontrolü, rapor ve oturum devri |
| ca-rust-architecture | Crate sınırları, dependency yönü, tipler ve Rust hata yönetimi |
| ca-treesitter-language | ABI, node/query doğrulama, dil adaptörü ve extraction testleri |
| ca-mcp-contract | Protokol yaşam döngüsü, DTO/JSON şemaları, stdio ve tool sözleşmeleri |
| ca-index-storage | SQLite, indeks nesilleri, tek yazıcı, iptal ve kurtarma |
| ca-graph-resolution | Kapsam/import çözümleme, aday ayrımı, grafik ve arama doğruluğu |
| ca-security-review | Dosya sınırları, sırlar, güvenilmeyen içerik ve kaynak limitleri |
| ca-release-validation | Gerçek test/benchmark kanıtı, platformlar ve güvenli dağıtım |

Kökteki `AGENTS.md` tüm aşamalar için değişmez kuralları taşır: planlanan özelliği
bitmiş gösterme, regex ile Tree-sitter yerine geçme, kullanıcı ayarlarını izinsiz
bozma, startup'ı indekslemeye bağlama, belirsiz çağrı hedefini kesin gösterme.
`SKILL.md` çalışma yöntemidir; `prompts/*.md` o aşamadaki görevdir. Sunucunun Aşama
13'te sunacağı MCP promptları ise son kullanıcının kod sorgulama şablonlarıdır.
Bu üç kavram birbirinden ayrıdır.

## 6. Önemli doğruluk kararları

### Sözdizimi, tam semantik değildir

`service.Save(invoice)` çağrısını bulmak ile gerçek çalışma zamanında hangi concrete
implementasyonun çağrıldığını bilmek aynı şey değildir. Sonuçlar `observed`,
`lexically_resolved`, `candidate`, `unresolved` ayrımını ve kaynak kanıtını korur.
Opsiyonel LSP kanıtı varsa `semantically_resolved` ayrıca kullanılabilir. Sayısal
“%99 güven” gibi ölçülmemiş skorlar üretme. Interface/trait, overload, DI ve dinamik
çağrılarda belirsizliği görünür tut. Aşama 17 olmadan da temel ürün kullanılabilir olmalıdır.

### Başlangıçta ağır iş yok

Codex sunucuyu başlattığında ayrı daemon beklenecek veya bütün repo indekslenecek bir
akış tasarlanmamıştır. Önce protokol/status çalışır; indeksleme açık bir çağrıyla başlar
ve `job_id` döner. `job_status` ile takip edilir, `cancel_job` ile işbirlikçi iptal edilir.
Bunlar uygulamaya ait iş araçlarıdır, MCP Tasks uyumluluğu iddiası değildir.

Aynı worktree'ye iki istemci bağlanırsa biri yazıcı olur, diğeri mevcut indeksi okur.
İkinci istemcide mutation bounded `WRITER_BUSY` döner; gizli daemon'a iş aktarılmaz.
Sahip süreç bittiğinde OS lock serbest bırakılır. Bu kısıt ilk sürümün bilinçli tercihidir.

### İndeks yarım kalırsa çalışan veri bozulmaz

Yeni indeks ayrı bir nesilde hazırlanır. Dosyalar, ilişkiler ve arama satırları birlikte
hazırsa aktif işaretçi değişir. İptal, okuma hatası veya süreç çökmesinde eski indeks
aktif kalır. Hash ile kaynağın değiştiği saptanırsa eski byte aralığını yeni dosyaya
uygulamak yerine `CONTENT_CHANGED` döner. Kullanıcı notları rebuild/GC'den ayrıdır.

### Dil desteği bir test matrisiyle kanıtlanır

Dart paket/fork isimleri veya sürüm numaraları birbirinin yerine kullanılamaz.
Query node isimleri seçilen grammar'ın gerçek `node-types` ve parse ağacından türetilir.
Records/patterns/extension types gibi modern yapılar ayrı yetenek testidir; kapsam dışı
olan yapı açıkça yazılır. Eksik zorunlu destek, “destekleniyor” etiketiyle saklanmaz.

## 7. Planlanan MCP araçları

`repository_status`, `index_repository`, `job_status`, `cancel_job`, `search_symbols`,
`get_symbol`, `find_references`, `trace_calls`, `get_file_outline`, `read_code`,
`get_repo_map`, `analyze_impact`, `build_context`, `search_memories`, `upsert_memory`,
`forget_memory`. Bütün giriş/çıkış ve izin kuralları [MCP_CONTRACT.md](docs/MCP_CONTRACT.md)
içindedir. Henüz uygulanmayan araç `tools/list` içinde gösterilmez.

Önerilen başlangıç limitleri: 20 sonuç varsayılanı, en fazla 200 sonuç, 64 KiB toplam
serileştirilmiş MCP yanıtı, 4 KiB snippet, en fazla 8 grafik derinliği ve 500 düğüm.
Bunlar ürün tasarım değerleridir; ölçülmüş hız veya token tasarrufu iddiası değildir.
JSON gövdesi byte ortasından kesilmez; öğe azaltılıp tekrar serileştirilir.

## 8. Codex entegrasyonu ve güvenlik

`config/codex.macos.example.toml` ve `config/codex.windows.example.toml` Aşama 15 için
örnektir. **Henüz binary olmadığı için bu örnekleri şimdi çalışan MCP diye ekleme.**
Komut ve kök dizin için gerçek mutlak yollar kullanılacaktır. `required = false`
seçimi CodeAtlas sorununun Codex oturumunun tamamını zorunlu olarak engellememesi içindir.

Planlanan `codeatlas integrate codex --dry-run` yalnız farkı gösterecek; `--apply`
açık talepte yedek alıp yalnız CodeAtlas girdisini değiştirecektir. Bu komutlar uygulama
hedefidir, bu kit hazırlanırken çalıştırılmış veya mevcut araçlar değildir. Eski
codebase-memory-mcp girdisini otomatik silmez ve diğer MCP sunucularını bozmaz.

İndeks veritabanı kaynak klasörünün dışında yerel uygulama veri dizininde tutulur.
Root dışına geçiş, symlink/junction, `.env` ve credential yolları varsayılan olarak
engellenir. Repo build/script/package restore çalıştırılmaz. Kaynak ve memory içeriği
talimat değil veri kabul edilir. Yanıtları kullanan uzak modelin/veri politikasının
ayrıca değerlendirilmesi gerekir; yerel indeks bunu otomatik çözmez.

## 9. Paket doğrulaması ile ürün testini ayır

`fixtures/` içinde 24 özgün seed kaynak dosyası bulunur. Bunlar tam doğruluk korpusu
değildir. `tests/acceptance-scenarios.json` içindeki 43 senaryo uygulanacak ürün
kabul tasarımıdır; tümünün başlangıç durumu NOT_EXECUTED'dır.

Kitin dosya bütünlüğünü kontrol etmek için Python 3.11+ ile:

```sh
python3 scripts/validate_kit.py
```

Windows'ta uygun Python launcher varsa `py -3.11 scripts/validate_kit.py` kullanılabilir.
Bu komut TOML/JSON, prompt/skill dosya eşleşmeleri, fixture varlığı ve güvenli örnek
varsayılanları denetler. Rust kodu derlemez, MCP başlatmaz, dilleri test etmez.
Rust uyumluluk testleri Aşama 00'da ayrıca çalıştırılmıştır. `KIT_VALIDATION.md` bu
statik kontrolün sonucunu, `docs/reports/00-compatibility-spike.md` ise çalıştırılan
Rust/MCP/SQLite kanıtını kaydeder.

**Şimdi uygulanacak dosya: `prompts/01-workspace-and-lifecycle.md`.**
