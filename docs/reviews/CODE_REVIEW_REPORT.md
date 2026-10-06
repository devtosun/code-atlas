# CodeAtlas — kanıta dayalı kod incelemesi

İnceleme tarihi: 2026-10-06. Kapsam: mevcut working tree; yalnız son diff değil.
Bu tur yalnız inceleme ve rapor üretimidir. Düzeltme uygulanmadı.

## A. Yönetici özeti

### İncelenen durum

- Repository: `/Users/devtosun/springrepo/mcp/codeatlas-mcp-codex-kit`.
- Branch: `main`.
- HEAD: `8461837e5cff3b002e8b8ffff5b5cd9793e4ba63`.
- Ortam: macOS 27.0, Darwin ARM64; Python 3.14.5.
- Commit edilmemiş Phase 15 kaynak, manifest, paketleme, CI ve dokümantasyon değişiklikleri kapsamda.
- Kök `AGENTS.md` okundu; ek alt-dizin `AGENTS.md` bulunmadı. İlgili mimari,
  Tree-sitter, çözümleme, storage, MCP ve güvenlik skill'leri okundu. Bunlar
  özellikle kesinlik etiketleri, nesil tutarlılığı ve negatif sınır testlerine yön verdi.
- Önerilen codebase-memory MCP keşif/indeks araçları oturumda erişilebilir değildi.
  Dosya sistemi ve doğrudan kaynak incelemesi kullanıldı. Uygulamanın indeksi tek
  doğruluk kaynağı yapılmadı; küçük örneklerin beklenen ilişkileri bağımsız belirlendi.

Başlangıç `git status --short`:

```text
 M .gitignore
 M Cargo.lock
 M Cargo.toml
 M README.md
 M README.tr.md
 M config/dependency-lock.json
 M crates/ca-cli/Cargo.toml
 M crates/ca-cli/src/main.rs
 M docs/DEPENDENCY_POLICY.md
 M docs/MCP_CONTRACT.md
 M docs/PROJECT_STATE.md
 M docs/SECURITY_PRIVACY.md
 M docs/TEST_MATRIX.md
 M scripts/validate_kit.py
?? .github/workflows/release-macos-arm64.yml
?? LICENSE-NOTICE.md
?? crates/ca-cli/src/codex_integration.rs
?? docs/COMMAND_REFERENCE.md
?? docs/adr/0013-native-package-and-owned-codex-config.md
?? docs/manual/15-codex-smoke.md
?? docs/release/
?? docs/reports/15-release-and-codex.md
?? docs/reports/artifacts/15-dependency-inventory.json
?? docs/reports/artifacts/15-package-smoke.json
?? scripts/package_macos.py
?? scripts/phase15_package_smoke.py
```

### Sonuç ve yayın kararı

13 doğrulanmış bulgu: **5 P1, 8 P2**. P0 düzeyinde, geniş etkili bir açık bu
incelemede doğrulanmadı; bu, incelenemeyen alanlar için güvence değildir.

Öncelikli yayın engelleri:

1. CR-001: sıradan, satır sonu olmayan bir Rust dosyası çözümlemeyi sonsuz scope
   döngüsüne sokabiliyor. İş tamamlanmıyor; döngü içinde cancellation kontrolü yok.
2. CR-002: mevcut dosya geçersiz UTF-8'e dönüştüğünde silinmiş sayılıyor ve sağlıklı
   aktif indeksin yerine eksik nesil başarıyla etkinleştiriliyor.
3. CR-003–005: alan referansları, Go paket sınırları ve yerel Rust import scope'ları
   yanlış hedeflere `lexically_resolved` ilişkiler üretiyor. Bu yalnız düşük recall
   değil; varsayılan referans/etki sonuçlarına yanlış kesinlik taşıyor.

Diğer somut riskler: eksik file-version kimliği, tüm parse sonuçlarını biriktiren
kuyruk, temizlenmeyen eski nesiller, Dart import filtrelerinin yok sayılması,
JS/TS arrow-function export bağlantılarının kaybı, sessiz arama kesilmesi, tam MCP
yanıt bütçesinin aşılması ve bozuk Codex config satırının stderr'e sızması.

Bu haliyle doğruluk/güvenilirlik açısından yayın onayı önerilmiyor. Teknik stdio
entegrasyonunun çalışması, analiz doğruluğu veya production-ready onayı değildir.

### Kanıtın sınırları

Güncel kaynak ağacına karşı Rust testleri **yeniden derlenemedi**: `cargo`/`rustc`
erişilemiyor; önceki geçici toolchain'in `cargo -> rustup` bağlantısının hedefi yok.
Yeni araç kurulmadı, ağ/izin genişletilmedi. Cargo kontrolleri BLOCKED'dır.

Dinamik testler mevcut `target/release/codeatlas` üzerinde, tamamen geçici
repository/HOME/SQLite verileriyle çalıştı. Binary SHA-256:

```text
45f11cecbadafc64456fd06ea36dcca9e140737d808f42ac56878afee6227b82
```

Bu hash Phase 15 kayıtlı binary ile aynı. Ancak bugünkü working tree'nin yeniden
derlenmiş çıktısı olduğu kanıtlanmadı. Bulguların güncel kaynakta bulunan yürütme
yolları ayrıca kontrol edildi. Mevcut binary testleri, güncel kaynak testlerinin
geçtiği şeklinde sunulmuyor. CR-007 kaynak/yürütme-yolu kanıtıdır; bellek benchmark'ı
değildir. CR-006'daki eski grammar durumu izole DB'de simüle edildi.

Olumlu kanıtlar: bütün grammar/query ve mevcut fixture kapıları, çıkarılmış paket
smoke'u, iki protokol dönemi, gerçek writer/follower okuması, temel incremental/full
eşitliği, export invalidation, rename/delete, UTF-8/CRLF byte konumları, dış-root/
symlink/ignore reddi, stale kaynak hash'i ve memory revision çatışması kontrolleri
çalıştı. Bunlar aşağıdaki negatif örnekleri kapsamıyor veya onların başarısızlığını
ortadan kaldırmıyor. Linux/Windows, yeni fuzz/audit, model-backed Codex oturumu ve
geniş concurrency/crash matrisi bu tur doğrulanmadı.

## B. Önceliklendirilmiş bulgular

Kanıt etiketleri:

- **D+K:** mevcut binary'de izole reproduction + güncel kaynak/yürütme-yolu incelemesi.
- **K:** güncel kaynak ve erişilebilir çağrı yolu; dinamik etki ölçülmedi.

Bulgu kökeni son commit/diff ile tarihsel olarak karşılaştırılmadı. Hiçbiri
kanıtsız biçimde “Phase 15'in getirdiği regression” olarak adlandırılmıyor.

### [P1] CR-001 — Eşit byte aralıklı scope'lar çözümlemeyi sonsuz döngüye sokuyor

- Konum: `crates/ca-engine/src/resolution.rs:1503-1515`, `:1520-1521`.
- İlgili sembol: `scope_chain`, `contains`; çağıran `Resolver::lexical_candidates`.
- Kanıt türü: D+K.
- Tetiklenme koşulu: dosya ve içerdiği fonksiyonun syntax aralığı eşit; örneğin
  sonuna LF konmamış `a.rs` dosyasının tamamı `fn main(){target();}`.
- Beklenen davranış: scope zinciri sonlu ve cycle-safe olmalı; tanımsız `target`
  unresolved kalmalı. `AGENTS.md`, `docs/ARCHITECTURE.md` ve ADR-0008 bounded,
  cooperative ve deterministik çözümleme gerektiriyor.
- Mevcut davranış: parent seçimi eşit aralıkları kabul ediyor. File scope function
  scope'u, function scope file scope'u parent seçiyor; visited/depth/cancellation
  kontrolü olmadan `Vec<String>` büyütülüyor. Üst seviyedeki cancellation kontrolü
  bu yardımcı fonksiyonun içindeki döngüyü kesemiyor.
- Etki: temel indeksleme bitmiyor; writer işi işgal ediliyor. Bellek büyümesi kaynak
  yolundan açık; RSS/OOM miktarı bu tur ölçülmedi. EOF/join de aynı worker'ı bekleyebilir.
- Kısa teknik kanıt / reproduction: geçici kökte `index --root ROOT --json` 1,5
  saniyede bitmedi; yalnız testin kendi subprocess'i öldürüldü. SQLite job durumu
  `resolving`; scope'lar `file[0,20)`, `function[0,20)`, `function_body[9,20)`.
  Eşit aralıklı iki dış scope'un birbirini seçmesi doğrudan kaynakta doğrulandı.
- Minimum düzeltme yaklaşımı: mümkünse AST parent ilişkisi kullan; eşit-range
  scope'lar için açık, acyclic tie policy tanımla. Zincire visited/depth bütçesi ve
  cancellation ekle. Yalnız `contains` eşitliğini kaldırmak kök scope'u kaybetmemeli.
- Regression testi: LF'siz tek fonksiyon, eşit-range nested scope ve boş dosya;
  unresolved çağrının sonlu sürede bitmesi; cancellation ve MCP EOF'nin worker'ı
  sonlandırması. JS/Dart benzer scope şekilleri de negatif fixture olmalı.
- Geriye uyumluluk / migration riski: scope çıkarımı değişirse extractor
  fingerprint'i yükseltilmeli; yalnız resolver değişirse mevcut graph yeniden
  çözülmeli. Eski scope ID'lerine bağlı sonuçların cursor/kanıt geçerliliği korunmalı.

### [P1] CR-002 — Okunamayan mevcut kaynak silinmiş sayılıp eksik indeks etkinleştiriliyor

- Konum: `crates/ca-engine/src/repository.rs:1060-1072`, `:886-897`;
  `crates/ca-engine/src/indexing.rs:494-520`.
- İlgili sembol: `FileScanner::visit_directory`, `scan_paths`, `IndexService::run_pipeline`.
- Kanıt türü: D+K; UTF-8 dinamik, oversize/binary aynı kaynak hata kolunda.
- Tetiklenme koşulu: önceden indekslenmiş desteklenen dosya hâlâ mevcut fakat
  geçersiz UTF-8, NUL/binary veya policy'den büyük içerik taşıyor.
- Beklenen davranış: `docs/SECURITY_PRIVACY.md:79-84` bu tür başarısızlıkların aday
  nesli iptal edip sağlıklı aktif veriyi koruyacağını açıkça söylüyor. “Dosya yok”
  ile “dosya analiz edilemedi” ayrılmalı; eski kaynak yeniymiş gibi de sunulmamalı.
- Mevcut davranış: bu üç reader hatası `scan.complete=false` yapmıyor. Dosya
  `scan.files` içine girmiyor. Pipeline complete taramadaki eksik path'i deletion
  sayıyor; yeni nesil `completed` oluyor.
- Etki: sağlıklı dosyanın sembol/graph/search üyeliği aktif sonuçlardan kayboluyor;
  `files_failed=0` başarı raporu yanıltıcı. Kaynak dosyası veya memory fiziksel olarak
  silinmiyor; kayıp aktif türetilmiş indeks üyeliğinde.
- Kısa teknik kanıt / reproduction: önce `pub fn healthy() {}\n` indekslendi;
  sonra aynı geçici `a.rs` `FF FE` byte'larına dönüştürüldü. İkinci index exit 0:
  `state=completed`, `files_discovered=0`, `files_failed=0`, `files_deleted=1`,
  yalnız invalid-UTF-8 warning'i. Sağlıklı nesil
  `g-000000000000000018dbf273f7bb5f88-0000000000000001`; yerine etkinleşen nesil
  `g-000000000000000018dbf273fa6205e8-0000000000000001`.
- Minimum düzeltme yaklaşımı: ignore/policy ile bilinçli exclusion ve analiz/read
  başarısızlığını ayrı scan outcome yap. Dokümante edilen politika gereği aday
  aktivasyonunu durdur; deletion inference'a başarısız path gönderme.
- Regression testi: geçerli dosyayı UTF-8 dışı, NUL ve limit üstü içeriğe çevir;
  hem full hem targeted scan'de aktif generation ID aynı kalsın, iş başarısız/
  incomplete olsun, gerçek deletion ise üyeliği kaldırsın. Notlar korunmalı.
- Geriye uyumluluk / migration riski: schema değişimi zorunlu değil; job hata
  davranışı değişir. Önceden kaybolan türetilmiş kayıtlar yeniden indekslenmeli;
  eski snippet hash kontrolünü gevşetmek çözüm değildir.

### [P1] CR-003 — Üye/alan referansları aynı adlı yerel değişkene kesin bağlanıyor

- Konum: `crates/ca-engine/src/resolution.rs:667-689`;
  `crates/ca-languages/src/adapters.rs:1408-1415`, `:1767-1774`, `:2160-2173`,
  `:2501-2528`, `:1079-1091`.
- İlgili sembol: `Resolver::lexical_candidates`, dil adaptörlerinin `decorate_reference`.
- Kanıt türü: D+K; Rust, Go, C#, Java ve Dart üzerinde ayrı örnekler.
- Tetiklenme koşulu: bir scope'ta `value` yerel değişkeni ve `obj.value` alan
  erişimi var. Capture, member-selector referansına receiver/uncertainty rolünü
  taşımıyor. Rust/Go decoration boş; C#/Java olağan alan erişimini ayırmıyor;
  Dart yalnız member'ın object tarafını işaretliyor.
- Beklenen davranış: `obj.value`, bare local `value` değildir. Tür çözümü yoksa
  candidate/unresolved korunmalı. LANGUAGE_SUPPORT ve ADR-0008 compiler dispatch
  iddiası olmadan lexical shadowing ve açık belirsizlik vaat ediyor.
- Mevcut davranış: resolver bare spelling üzerinden en yakın scope'taki local'i
  seçiyor; `lexical-scope-v1`, `lexically_resolved` edge yazıyor.
- Etki: `find_references(include_candidates=false)`, impact ve context yanlış
  local'e ait kullanımlar gösterebilir. Genel syntax-only etiketi yanlış kesin
  ilişkiyi düzeltmiyor.
- Kısa teknik kanıt / reproduction:

  ```rust
  struct S { value: i32 }
  fn read(obj: S) { let value=9; let x=obj.value; }
  ```

  Bağımsız SQLite sorgusu aktif nesilde şu referans başlangıcı → yanlış local
  declaration başlangıcını verdi (zero-based UTF-8 byte): Rust `65 → 46`, Go
  `73 → 57`, C# `70 → 50`, Java `63 → 43`, Dart `65 → 45`. Hepsi
  `lexically_resolved`. Go'daki ayrı bare `Value` referansı `82 → 57` doğru;
  yanlış olan member selector'ı. Tekrarlanan testlerde sorgu active generation
  ile filtrelendi; eski nesil kopyaları duplicate-app-bug sayılmadı.
- Minimum düzeltme yaklaşımı: her adaptörde member property'sini bare identifier
  rolünden ayır; receiver/source ve belirsizlik aktar. Lexical aday koluna girmesini
  engelle; static import-prefix/member ayrımı kanıtlanamıyorsa kesinlik üretme.
- Regression testi: beş dilde aynı isimli field/local, bare-local pozitif kontrol,
  nested shadowing, `this/self` ve import prefix negatifleri; member edge hiçbir
  durumda ilgisiz local'e kesin bağlanmamalı.
- Geriye uyumluluk / migration riski: extractor fingerprint bump ve tam reparse/
  re-resolution gerekir; eski yanlış graph'ın yalnız yeni query ile okunması yetmez.

### [P1] CR-004 — Go package adı farklı dizinlerde tek paket kimliği sayılıyor

- Konum: `crates/ca-engine/src/resolution.rs:781-809`.
- İlgili sembol: `Resolver::same_module_candidates`, `declared_module`.
- Kanıt türü: D+K.
- Tetiklenme koşulu: root içindeki farklı dizinler aynı `package` clause'unu kullanıyor.
- Beklenen davranış: Go'da farklı dizinler aynı package adına sahip olsa da aynı
  lexical paket değildir. LANGUAGE_SUPPORT'taki bounded same-module/package
  kuralları dosya/dizin kimliğini korumalı; compiler/SDK çalıştırmak gerekmiyor.
- Mevcut davranış: dil ve `declared_module` string'i eşitse bütün target dosyaları
  taranıyor; directory/module-root karşılaştırılmıyor. Tek isim eşleşmesi kesin sayılıyor.
- Etki: repository büyüdükçe ilgisiz pakete cross-file çağrı/referans üretiliyor;
  yanlış dependency ve etki analizi ana işleve yayılıyor.
- Kısa teknik kanıt / reproduction:

  ```text
  a/a.go: package shared\nfunc OnlyA() {}\n
  b/b.go: package shared\nfunc Run() { OnlyA() }\n
  ```

  Aktif DB edge'i: `b/b.go:OnlyA → a/a.go:OnlyA`, `lexically_resolved`,
  `same-module-v1`. `b` paketinde erişilebilir `OnlyA` yok.
- Minimum düzeltme yaklaşımı: Go paket kimliğine authorized root/module identity ve
  kaynak directory'sini kat; aynı-dizin tanımlarını ayır. Build-tag seçimi belirsizliğini
  ayrıca koru; yalnız package adıyla farklı dizinlere geçme.
- Regression testi: aynı package adı/farklı directory negatif, aynı directory pozitif,
  farklı root/module, `_test` package ve build-tag belirsizlik örnekleri.
- Geriye uyumluluk / migration riski: resolver rule/version bump; bütün aktif graph
  yeniden çözülmeli. Kullanıcı notlarını/symbol source kimliklerini gereksiz değiştirme.

### [P1] CR-005 — Fonksiyon içindeki Rust import'u kardeş fonksiyona sızıyor

- Konum: `crates/ca-engine/src/resolution.rs:744-765`.
- İlgili sembol: `Resolver::imported_candidates`, `import_matches_occurrence`.
- Kanıt türü: D+K.
- Tetiklenme koşulu: bir Rust fonksiyonundaki block-scoped `use`, başka fonksiyondaki
  aynı bare ismi etkiliyor. Çıkarım `import.scope_id` üretse de resolver kullanmıyor.
- Beklenen davranış: yalnız occurrence'ın erişilebilir scope zincirindeki import'lar
  aday olmalı; lexical binding/alias shadowing korunmalı. ADR-0008 lexical/import
  kurallarını belirtiyor.
- Mevcut davranış: dosyanın tüm Import gözlemleri filtrelenip aynı ada uygulanıyor;
  scope ilişkisi ve import önceliği kontrol edilmiyor.
- Etki: tanımsız isimler ilgisiz import'a kesin bağlanıyor; yanlış caller/callee ve
  referans sonuçları oluşuyor. Bu bulguda diğer dillerde benzer etki iddia edilmiyor.
- Kısa teknik kanıt / reproduction:

  ```rust
  // src/lib.rs
  mod other;
  fn inside() { use crate::other::target; target(); }
  fn outside() { target(); }
  // src/other.rs
  pub fn target() {}
  ```

  Her iki call-site da `src/other.rs:target` hedefli, `lexically_resolved`,
  `rust-module-path-v1`; `outside` için import scope'u erişilebilir değil.
- Minimum düzeltme yaklaşımı: import adaylarını scope-chain membership ve nearest
  binding'e göre seç; scope dışı import'u uygulama. CR-001'deki cycle-safe scope
  zinciri bu işin önkoşulu.
- Regression testi: kardeş fonksiyonlar, nested block, dış/yerel alias shadowing;
  yalnız erişilebilir import resolve olsun, kardeş scope unresolved kalsın.
- Geriye uyumluluk / migration riski: resolver sürümünü değiştirip graph'ı yeniden
  çöz; schema değişimi zorunlu değil. Daha önceki kesin ilişkilerin azalması beklenir.

### [P2] CR-006 — Reuse reddedilse bile storage eski file-version kimliğini yeniden kullanıyor

- Konum: `crates/ca-storage/src/schema.rs:31-38`;
  `crates/ca-storage/src/storage.rs:2867-2888`, `:2896-2900`;
  `crates/ca-languages/src/registry.rs:61-75`.
- İlgili sembol: `stage_file`, `LanguageProvider::extractor_fingerprint`;
  çağıran `IndexService::parse_and_stage` (`indexing.rs:695-704`).
- Kanıt türü: D+K; eski grammar metadata'sı izole DB'de simüle edildi.
- Tetiklenme koşulu: source/query/adapter-version aynı kalırken grammar fingerprint
  değişiyor. Config fingerprint değişimi de reuse reddi üretiyor fakat bu kimliğe
  dahil değil; bu ikinci varyant dinamik olarak denenmedi.
- Beklenen davranış: DATA_MODEL'ın immutable file-version sözleşmesi ve ayrı
  grammar/query/extractor/config karşılaştırması gereği yeni analiz identity/provenance
  ile saklanmalı; sonraki incremental run bunu reuse edebilmeli.
- Mevcut davranış: UNIQUE yalnız `(file_id,content_hash,extractor_hash)`.
  Extractor hash query+adapter-version'dan oluşuyor, grammar içermiyor. Reparse sonrası
  INSERT conflict `DO NOTHING`; SELECT eski ID'yi döndürüyor. Eski grammar/coverage
  metadata'sı kalıyor; observations/facts aynı eski version'a yazılmaya devam ediyor.
- Etki: grammar-only upgrade'de sürekli gereksiz reparse ve yanlış analiz provenance.
  Yeni parser farklı facts üretirse eski immutable snapshot'ın facts kümesini
  genişletme/eski facts'i koruma riski kaynak yolunda var; bu son etki değişmiş native
  grammar ile dinamik olarak ölçülmedi ve gözlenmiş veri bozulması diye sunulmuyor.
- Kısa teknik kanıt / reproduction: geçici `a.rs` indekslendi; yalnız geçici DB'de
  `grammar_hash='previous-grammar-fingerprint'` yapılarak eski cache simüle edildi.
  İki incremental run da `parsed=1,reused=0`; DB'de tek version ID `1` ve eski grammar
  hash'i kaldı. Gerçek kullanıcı DB'sine veya dependency'ye dokunulmadı.
- Minimum düzeltme yaklaşımı: immutable analiz kimliğine tüm ilgili fingerprint'leri
  kat; yeni key için migration/new-version oluştur. Eski version satırını yerinde
  UPDATE ederek snapshot immutability'yi bozma.
- Regression testi: aynı source/query ama değişen grammar ve değişen extractor-config;
  yeni version/provenance oluşsun, eski facts sabit kalsın, ikinci run reuse etsin.
  Farklı fact üreten test extractor'ı ile tarihsel snapshot izolasyonu doğrulanmalı.
- Geriye uyumluluk / migration riski: uniqueness/index migration gerekiyor. Mevcut
  graph foreign key'leri, memory evidence ve eski cursor'lar korunmalı veya açıkça
  stale yapılmalı; sadece DB'yi silmek kabul edilebilir migration değildir.

### [P2] CR-007 — Sınırlı source kuyruğuna rağmen bütün parse sonuçları bellekte birikiyor

- Konum: `crates/ca-engine/src/indexing.rs:643-646`, `:674-675`, `:683-744`.
- İlgili sembol: `IndexService::parse_and_stage`.
- Kanıt türü: K. Peak RSS/latency benchmark'ı yapılmadı.
- Tetiklenme koşulu: çok sayıda yeni/değişmiş dosyanın bir işte parse edilmesi.
- Beklenen davranış: AGENTS/ADR-0007: her kuyruk bounded; küçük batch'ler persist
  edilmeli, tüm facts bellekte tutulmamalı. Source kapasitesi tek başına yeterli değil.
- Mevcut davranış: `source_tx` sync_channel iken `result_tx` unbounded channel.
  Worker'lar owned `IndexedFile` gönderiyor. Coordinator tüm source'ları gönderip
  bitirmeden result queue'yu okumuyor. Persist batch limiti bu birikimden sonra devreye giriyor.
- Etki: resident sonuç belleği toplam parse edilen facts/diagnostics büyüklüğüne
  bağlı; configured queue capacity'ye bağlı değil. Büyük işte bellek baskısı ve
  geç ilk-persist riski somut tasarım yolunda; “ölçülmüş OOM” iddia edilmiyor.
  `max_files` bir işin sonlu olmasını sağlar, bounded working set sağlamaz.
- Kısa teknik kanıt / reproduction: `run_pipeline → parse_and_stage → worker.extract
  → sender.send` yolu; consumer ilk kez source for-loop bittikten sonra
  `result_rx.recv_timeout` çağırıyor. Daha önce bu yolu kesen bir drain bulunmadı.
- Minimum düzeltme yaklaşımı: production ve consumption'ı eşzamanlı koordine et;
  result queue'ya da backpressure ve cancellation koy; batches'i iş sürerken persist
  et. Yalnız channel'ı sync_channel yapmak, mevcut sırada iki yönlü deadlock doğurabilir.
- Regression testi: fake extractor ile çok sayıda büyük sonuç üret; high-water
  buffered result sayısı policy kapasitesini aşmasın. Slow DB, worker failure,
  cancellation ve sender/receiver kapanışı deadlock üretmemeli. Sonra native RSS ölç.
- Geriye uyumluluk / migration riski: schema/API değişimi gerekmeyebilir; staging
  sıralaması deterministik olmalı, failed generation aktivasyonu engellenmeye devam etmeli.

### [P2] CR-008 — Başarılı eski nesiller ve bağlı dosya sürümleri hiçbir zaman temizlenmiyor

- Konum: `crates/ca-storage/src/storage.rs:3291-3297`, `:3361-3374`.
- İlgili sembol: `activate_generation`, `gc_abandoned`.
- Kanıt türü: D+K.
- Tetiklenme koşulu: tekrar indeksleme veya watcher altında uzun süreli düzenleme.
- Beklenen davranış: DATA_MODEL cursor retention window/yerel storage limitleri ve
  SECURITY_PRIVACY DB growth sınırı vaat ediyor. Active/previous korunurken daha eski
  nesiller için sonlu retention uygulanmalı; memory ayrı kalmalı.
- Mevcut davranış: eski active `superseded` oluyor; GC yalnız `abandoned` siliyor.
  Superseded generation membership bütün eski versions'ı canlı tutuyor. Tüm source
  yollarında başka retention/GC çağrısı veya configurable generation/storage sınırı bulunmadı.
- Etki: generations, memberships, graphs ve değişen file versions birikiyor;
  no-change indeks bile yeni generation üretir. Uzun watcher kullanımında disk/DB
  büyümesi sınırlanmıyor. Büyük ölçekli disk büyüme oranı benchmark yapılmadı.
- Kısa teknik kanıt / reproduction: tek geçici dosya beş farklı içerikle beş kez
  indekslendi; `active=1`, `superseded=4`, `file_versions=5`. Kaynakta bu başarılı
  tarihsel nesilleri collectible yapan yol yok.
- Minimum düzeltme yaklaşımı: açık cursor retention ve storage quota policy'si;
  active/izin verilen previous nesilleri pinle, daha eskilerini kısa writer transaction'ında
  collect et. `generations.parent_id` foreign key zinciri güvenle ele alınmalı.
- Regression testi: yüzlerce edit ve no-change run; retained generation/version/edge
  sayısı sınırda kalsın, active ve geçerli cursor çalışsın, expired cursor `STALE_CURSOR`
  dönsün, kullanıcı memories ve evidence hiçbir zaman orphan/purge edilmesin.
- Geriye uyumluluk / migration riski: eski cursor'ların ömrü değişir; bu belgelenmeli.
  FK-safe GC/migration gerekli olabilir; gerçek kullanıcı DB'sini topluca silme çözüm değil.

### [P2] CR-009 — Dart `show`/`hide` filtreleri bağlama uygulanmıyor

- Konum: `crates/ca-languages/src/adapters.rs:894-916`;
  `crates/ca-engine/src/resolution.rs:1589-1593`.
- İlgili sembol: `DartAdapter::imports`, `import_matches_occurrence`.
- Kanıt türü: D+K; `show` dinamik, `hide` aynı eksik filtre yolu.
- Tetiklenme koşulu: yerel relative Dart import'u combinator ile isimleri sınırlandırıyor.
- Beklenen davranış: dar statik import kuralları yasaklanmış adı kesin bağlamamalı.
  Show/hide tam desteklenmeyecekse bu form açık unsupported/unresolved olmalı;
  analyzer veya paket restore gerektirmez. LANGUAGE_SUPPORT relative import desteği vaat ediyor.
- Mevcut davranış: adapter URI/alias/conditional metadata'sını topluyor, show/hide
  üyeliği taşımıyor. Resolver yalnız qualifier ve alias'a bakıp her ismi kabul ediyor.
- Etki: görünür olmayan fonksiyon için kesin caller/callee; yanlış impact/context.
- Kısa teknik kanıt / reproduction: `a.dart` içinde `allowed` ve `hidden`; `b.dart`:
  `import 'a.dart' show allowed;\nvoid run() { hidden(); }\n`. Aktif edge:
  `hidden → a.dart:hidden`, `lexically_resolved`, `dart-relative-library-v1`.
- Minimum düzeltme yaklaşımı: import combinator gözlemlerini normalize et ve aday
  seçmeden önce uygula; export traversal'da da filtreyi koru. Desteklenmeyen formda
  explicit limitation + unresolved/candidate kullan.
- Regression testi: show, hide, prefix, zincirli/re-export filtreleri; excluded
  adlara kesin edge yazılmasın, allowed ad pozitif resolve olsun.
- Geriye uyumluluk / migration riski: extractor/resolver fingerprint bump;
  eski import gözlemlerinin reparse edilmesi ve graph'ın yeniden çözülmesi gerekir.

### [P2] CR-010 — Export edilmiş arrow-function binding'i JS/JSX/TS/TSX import'unda kayboluyor

- Konum: `crates/ca-languages/src/adapters.rs:2667-2671`;
  `crates/ca-engine/src/resolution.rs:1660-1664`.
- İlgili sembol: `EcmaAdapter::declaration_from_name`, `is_module_visible`.
- Kanıt türü: D+K; dört provider üzerinde ayrı reproduction.
- Tetiklenme koşulu: `export const work = () => 1;` başka dosyadan named-import ile çağrılıyor.
- Beklenen davranış: LANGUAGE_SUPPORT function bindings, ESM imports/exports ve
  relative module binding'i vaat ediyor. Basit export wrapper statik olarak görülebilir.
- Mevcut davranış: `exported` ancak syntax node'un doğrudan parent'ı
  `export_statement` ise ekleniyor. Variable declarator ile export arasındaki lexical
  declaration wrapper atlanmıyor. Resolver exported attr olmayan sembolü görünür saymıyor;
  export gözlemi bu basit yerel binding'in görünürlüğünü onarmıyor.
- Etki: yaygın JS/TS callable declaration'ları import üzerinden unreachable görünür;
  caller/callee/reference ve context recall düşer. Bu bulgu false-positive değil false-negative.
- Kısa teknik kanıt / reproduction: her `.js/.jsx/.ts/.tsx` çiftinde `a` yukarıdaki
  export; `b` içeriği `import { work } from './a';\nexport function run() { work(); }\n`.
  Dördünde de aktif edge `work`, `unresolved`, `no-supported-binding-v1`; hedef null.
- Minimum düzeltme yaklaşımı: declaration wrapper/export ownership'ini doğru izle
  veya explicit local export map'i binding'e uygula; nested declaration'ı yanlış
  exported sayma. Runtime/dynamic export çözümü kapsam genişletmesi değildir.
- Regression testi: named function, arrow/function-expression const, local export
  alias, default export pozitifleri; export edilmeyen/nested binding negatifleri.
  JSX ve TSX ayrı grammar'larla çalışmalı.
- Geriye uyumluluk / migration riski: extractor fingerprint bump, ilgili dosyaların
  reparse'i ve full-generation resolution gerekir. Yeni graph edge'leri oluşacak.

### [P2] CR-011 — Search cursor 10.000 adayda duruyor, kesilmeyi gizliyor

- Konum: `crates/ca-storage/src/storage.rs:3976`, `:4026-4027`, `:4146-4149`;
  `crates/ca-engine/src/retrieval.rs:607-619`.
- İlgili sembol: `ReadSnapshot::retrieval_search_symbols`, `RetrievalService::search_symbols`.
- Kanıt türü: D+K; küçük ve büyük sonuç kümeleri ayrı denendi.
- Tetiklenme koşulu: query için SQL candidate cap'inden fazla indexed declaration.
- Beklenen davranış: arama bounded olabilir; fakat kesilme açık bildirilmeli.
  MCP_CONTRACT `truncated`/`next_cursor` sözleşmesi ve ADR-0009 bounded candidate
  tercihi, eksik kümenin tamamlanmış gibi sunulmasına izin vermiyor.
- Mevcut davranış: SQL LIMIT cursor koşulundan önce uygulanıyor. Her sayfa aynı
  capped prefix'i getiriyor, `after` bellekte filtreleniyor. Son prefix sayfasında
  kalan indexed kayıtlar bilinmediği için `has_more=false`, cursor null oluyor.
- Etki: varsayılan istemci bütün sonuçları tükettiğini sanıyor; aynı query için
  limit'i yükseltmek eksik kayıtları erişilebilir yapmıyor.
- Kısa teknik kanıt / reproduction: 110 geçici Rust dosyası, her birinde 100 ayrı
  modülde `pub fn common() {}`: aktif indekste 11.000 common. `search_symbols`,
  `limit=200`, dönen cursor'lar ile sonuna kadar: 170 sayfa, 10.000 unique ID,
  terminal `truncated=false`, `next_cursor=null`. Sayfa boyunu byte budget azaltıyor.
  40 dosya/4.000 common kontrolü 68 sayfada 4.000 unique sonucu tamamladı.
  Bu çalışma latency benchmark'ı değil; süre/p95 sayısı çıkarılmadı.
- Minimum düzeltme yaklaşımı: bounded SQL keyset predicate'ini limit öncesine taşı;
  mümkün değilse candidate-limit hit bilgisini üst katmana geçir ve honest
  truncation/limitation döndür. Bitmiş gibi gösteren sahte continuation üretme.
- Regression testi: cap+1 ve çok daha fazla same-name symbol; bütün cursor'larla
  tam sonuç veya açık incomplete sinyali. Case/prefix/FTS dedup, filtre ve byte
  budget altında duplicates ve kayıt atlama olmamalı.
- Geriye uyumluluk / migration riski: SQL/index ve cursor formatı değişebilir;
  eski cursor'ları version kontrolüyle reddet, query generation'ını pinlemeyi koru.

### [P2] CR-012 — Tam MCP frame sınırı prompt girdisi ve uzun request ID ile aşılabiliyor

- Konum: `crates/ca-mcp/src/lib.rs:492-503`, `:776-788`, `:908-910`.
- İlgili sembol: `plan_change_prompt`, `investigate_failure_prompt`, `bounded_result`.
- Kanıt türü: D+K; modern 2026-07-28 metadata'lı istekler.
- Tetiklenme koşulu: uzun prompt argument veya string JSON-RPC request ID.
- Beklenen davranış: MCP_CONTRACT `65.536 UTF-8 byte`, protocol wrapper ve text
  fallback dahil tam response bütçesi. Prompt şablonları da bounded olarak belgelenmiş.
- Mevcut davranış: prompt string'leri boyutsuz echo edilerek `GetPromptResult`
  oluşturuluyor. Tool budget yalnız `CallToolResult` + sabit reserve ölçüyor;
  SDK'nin echo ettiği değişken uzunluktaki ID hesaba katılmıyor.
- Etki: kontrollü girişler bile client frame limitini aşabiliyor; hata/başarı yanıtı
  geçerli JSON olsa da sözleşme ihlali ve istemci reddi var. Bu tur crash görülmedi.
- Kısa teknik kanıt / reproduction: 70.000 ASCII karakterli `plan_change.objective`
  sonucunun gerçek stdout satırı **70.747 byte**; 70.000 karakterli ID ile
  `tools/call(repository_status)` **72.732 byte**. İkisi de valid JSON; iki örnekte
  secret veya kaynak kodu bulunmuyor. Geçerli modern `_meta` kullanıldı.
- Minimum düzeltme yaklaşımı: prompt girdisi ve serialize edilmiş prompt result'ını
  bütçele; SDK uyumlu ingress/frame/request-ID sınırı veya envelope-aware bütçe
  politikası koy. Büyük ID'yi aynen echo eden error da sınırı aşabilir: transport'un
  desteklediği kontrollü ret/kapanış davranışı tanımlanmalı. Protokolü elle yazma.
- Regression testi: her iki protokol dönemi; uzun ID, objective/failure/scope,
  Unicode ve JSON escaping. Başarılı/hatalı her emitted frame sınırda kalsın;
  fazla giriş bounded error veya belgelenmiş kontrollü ret ile sonuçlansın.
- Geriye uyumluluk / migration riski: istemciye açık input policy değişir; uzun
  argument/ID kullanımını dokumente et. DB migration gerekmiyor.

### [P2] CR-013 — Bozuk Codex TOML diagnostic'i gizli config satırını stderr'e yazıyor

- Konum: `crates/ca-cli/src/codex_integration.rs:50-54`, `:163-168`;
  `crates/ca-cli/src/main.rs:581-585`.
- İlgili sembol: `CodexIntegrationError::Malformed`, `parse_document`, `main`.
- Kanıt türü: D+K; yalnız sentetik canary kullanıldı.
- Tetiklenme koşulu: Codex config'in unrelated bölümünde secret içeren hatalı TOML
  satırı; örneğin kapanış quote'u eksik değer. Dry-run da bu parsing'i yapıyor.
- Beklenen davranış: hatalı config fail-closed reddedilmeli, ancak unrelated
  kullanıcı config içerikleri diagnostic/diff'e taşınmamalı. ADR-0013/SECURITY_PRIVACY
  surgical diff'in özel değerleri açıklamamasını amaçlıyor.
- Mevcut davranış: TomlError Display offending source line/caret içeriyor;
  thiserror `source`'u formatlıyor, CLI bunu stderr'e yazıyor. Başarı diff'i sadece
  CodeAtlas table'ını gösterse de hata yolu aynı gizlilik sınırını sağlamıyor.
- Etki: yerel terminal/log veya agent'ın command output'u unrelated credential
  değeri içerebilir. Ağ üzerinden sunucunun kendiliğinden secret göndermesi gözlenmedi.
- Kısa teknik kanıt / reproduction: geçici config'e sentetik credential ve eksik
  closing quote kondu; `integrate codex --root ROOT --config TEMP --dry-run` exit 1;
  test yalnız `synthetic_secret_in_stderr=true` yazdı. Canary değeri rapora/log
  çıktısına kopyalanmadı; gerçek config okunmadı/değiştirilmedi.
- Minimum düzeltme yaklaşımı: dışarıya category/line/column gibi sanitize edilmiş
  diagnostic ver; source text taşıyan TomlError'u doğrudan formatlama/loglama.
  Redaction'ı yalnız `token` anahtarına bağlama; generic özel değerleri de koru.
- Regression testi: malformed config'in farklı bölümlerindeki canary'lerin
  stdout/stderr/error zincirinde bulunmadığını, original bytes'in değişmediğini
  doğrula; geçerli ownership/round-trip davranışını koru.
- Geriye uyumluluk / migration riski: yalnız diagnostic ayrıntısı azalır; CLI exit
  code/error category korunabilir. DB veya gerçek Codex config migration'ı yok.

## C. Doğrulanmamış şüpheler ve ayrı test boşlukları

Bunlar B'deki kesin hata sayısına dahil değildir.

1. **Conditional/configuration-dependent bindings.** Dart adapter conditional URI
   için `active_import_configuration_not_inferred` limitation'ı üretiyor
   (`adapters.rs:910-916`). Rust cfg/Go build tags/C# preprocessor da extraction'da
   var. Her resolver dalında bu belirsizliğin certainty'ye etkisi eksiksiz
   doğrulanmadı. Aynı isimli aktif/pasif tanımlar ve conditional Dart imports için
   bağımsız expected graph, source occurrence ve response certainty testleri gerekli.
2. **Codex config concurrent-edit/TOCTOU.** `write_atomic_with_backup` önce okunmuş
   original bytes'i kullanıyor; atomic rename tek başına başka editörün aradaki
   değişikliğini korumaz. Son read/rename arasındaki exact race bu tur tetiklenmedi.
   Kontrollü iki-yazıcı testinde CAS/lock davranışı, symlink replacement ve backup'ın
   hangi sürümü taşıdığı kanıtlanmalı. Gerçek kullanıcı config'inde denenmemeli.
3. **FTS same-name join çoğalması ve derin cursor maliyeti.**
   `storage.rs:4103-4108` document→symbol eşlemesini version+spelling ile yapıyor;
   aynı isimli birden çok declaration/document çarpan oluşturabilir. CR-011 sonuç
   kaybı doğrulandı; ayrıca p95/RSS/SQL row maliyeti veya belirli yanlış FTS hit'i
   ölçülmedi. Query-plan ve 10k/100k, same-name, signature-only eşleşme benchmark'ı
   ile ayrı performans/doğruluk kanıtı gerekli.
4. **Dar Rust module-path kuralları.** `resolve_rust_module` özel filesystem
   heuristics kullanıyor. Birden fazla `super`, inline modules, root-item imports
   ve directory/mod.rs eşlemesi bu tur kapsamlı negatif test edilmedi. Her şekil
   için bağımsız expected source-module graph gereklidir; compiler-level çözüm
   vaat edilmeyen durumları otomatik bug saymamak gerekir.
5. **Package/legal-policy tamlığı ve yeni advisories.** Locked inventory ve notices
   üretimi incelendi; ancak tüm vendored/native/transitive kaynakların notice
   yükümlülüğü tek tek ve yeni advisory verisiyle doğrulanmadı. Cargo audit/deny
   erişilemedi. Kayıtlı eski scan'in temiz olması bugünkü scan'in geçtiği değildir.
   Bu bir doğrulanmış lisans ihlali tespiti veya hukuki değerlendirme değildir.

Ayrı doğrulama boşlukları:

- `scripts/phase15_package_smoke.py` içindeki owner/follower bölümü yalnız
  initialize/tools-list/repository-status çağırıyor. Status cache-only
  (`mcp_backend.rs:136-153`), storage'ı açmıyor. Script'in `writer_follower=pass`
  etiketi gerçek writer contention/follower query kanıtı değildir. Bu tur ayrıca
  iki süreçte `search_symbols` çağrılıp `owner/read_only_follower` ve follower
  `WRITER_BUSY` doğrulandı. Test boşluğu uygulama follower bug'ı diye sayılmadı.
- Query/golden kapıları extraction doğrular; bunların geçmesi bütün resolver
  invariants'ın geçtiği anlamına gelmez. CR-003–005, CR-009–010 bunun örnekleridir.
- Fuzz target'ları parse, extraction ve input boundary için incelendi; bu tur
  fuzz çalıştırılmadı. ASan/TSan, native scanner fault isolation ve uzun soak yok.
- İndeksleme sırasında paralel snapshot okumaları, durable cancellation,
  crash/recovery ve migration fault injection için mevcut test yolları tarandı;
  güncel unit/integration testleri çalıştırılamadığı için yeniden onay verilmedi.
- Linux/Windows native path/junction/locking/packaging yok. macOS sonuçları o
  platformlara aktarılmadı. Unsigned/unnotarized paket belgelenmiş sınırlılık;
  tek başına bu incelemenin bug bulgusu değil.
- 0644 original config'in backup'ı da 0644 kaldı. ADR-0013 açıkça original mode'u
  benimsemeyi seçtiği için bu durum burada bağımsız bug sayılmadı; hassas config
  izin politikası ayrıca değerlendirilmelidir.

## D. Test ve komut sonuçları

### Ortam ve izolasyon

Bu bölümde kısaltmalar:

```text
W = /Users/devtosun/springrepo/mcp/codeatlas-mcp-codex-kit
T = /private/tmp/codeatlas-review-Qs9YG5
C = /private/tmp/codeatlas-review-Qs9YG5/worktree
B = /Users/devtosun/springrepo/mcp/codeatlas-mcp-codex-kit/target/release/codeatlas
```

`W` değişmeden `rsync -a` ile `C` oluşturuldu; `.git`, `target`, `dist`, `.codex`
ve Python cache'leri dışarıda bırakıldı. Rust kontrolleri `C`'de, ayrı target
directory ve offline ayarıyla denendi. Reproduction script'i yalnız `T` içinde
oluşturuldu; fixture'lar, HOME/config ve DB'ler `T` altında. Paket smoke kendi
temporary directory'sini kullandı. Test amaçlı SQL metadata değişimi yalnız
CR-006'nın geçici DB'sine yapıldı. Gerçek index/config veya source değişikliği yok.

Git kökü/HEAD/status, manifestler, CI, build/test script side effect'leri önce
incelendi. Üretim modunda repository build/restore/execute çağrısı bulunmadı;
`ca-cli/build.rs`'nin rustc sorgusu build-time. `--all-features` kullanılmadı.

### Güncel kaynak kontrolleri

| Komut | Çalışma dizini | Exit | Sonuç |
|---|---|---:|---|
| `git branch --show-current`, `git rev-parse HEAD`, `git status --short` | W | 0 / 0 / 0 | Kimlik ve dirty state kaydedildi; Git mutation yok |
| `rsync -a --exclude=.git --exclude=target --exclude=dist --exclude=.codex --exclude=__pycache__ ./ C/` | W | 0 | İzole kaynak kopyası |
| `cargo fmt --all -- --check` | C | 127 | Çalıştırılamadı: `command not found: cargo` |
| `cargo check --workspace --all-targets --locked --offline` | C | 127 | Çalıştırılamadı: aynı tooling eksikliği |
| `cargo clippy --workspace --all-targets -- -D warnings` | C | 127 | Çalıştırılamadı; lint sonucu değil |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | C | 127 | Çalıştırılamadı; istenen locked varyant da denendi |
| `cargo test --workspace --locked --offline` | C | 127 | Çalıştırılamadı; güncel Rust testleri PASS değil |
| `cargo audit --version` / `cargo deny --version` / `cargo fuzz --version` | C | 127 / 127 / 127 | Araç erişimi yok; advisory/license/fuzz tekrarları yapılamadı |
| `PYTHONDONTWRITEBYTECODE=1 python3 scripts/validate_kit.py` | C | 0 | Geçti: 18 prompt, 8 skill, 3 TOML, 24 fixture, 43 senaryo, 85 Markdown; Rust execution içermez |
| `git diff --check` | W | 0 | Mevcut tracked diff'te whitespace error yok; untracked rapor için Rust/lint kanıtı değil |
| `rsync -anic --omit-dir-times --exclude=.git --exclude=target --exclude=dist --exclude=.codex --exclude=__pycache__ --exclude=docs/reviews C/ W/` | W | 0 | Dry-run checksum karşılaştırması boş çıktı: başlangıç kopyasındaki mevcut dosyaların içerikleri korundu |
| Son `git status --short` | W | 0 | Başlangıç değişiklikleri aynı; yalnız yeni `docs/reviews/` eklendi; branch/HEAD değişmedi |

Eski `/private/tmp/codeatlas-cargo` ve `/private/tmp/codeatlas-rustup` PATH'iyle
denemeler de cargo'yu başlatamadı. Dosya/bağlantı kontrolü hedef toolchain'in
mevcut olmadığını gösterdi. Kurulum veya permission escalation yapılmadı.

CI'nın mevcut gerçek verify komutu `cargo run -p xtask -- verify`;
release workflow'u `cargo run -p xtask --locked -- verify` kullanıyor.
`xtask::VERIFY_GATES` fmt, strict clippy, locked workspace test çalıştırıyor;
sonra grammar ve dil/real-corpus kapıları geliyor. Bu tam verify zinciri bu tur
çalıştırılamadı. Mevcut Phase 15 raporundaki geçmiş 116 test sonucu buraya yeni
kanıt olarak taşınmadı.

### Mevcut binary ve paket kontrolleri

| Komut | Çalışma dizini | Exit | Sonuç/kanıt sınırı |
|---|---|---:|---|
| `target/debug/xtask grammar-check` | W | 0 | 9 provider, ABI 14/15, dört embedded query/provider doğrulandı; binary yeniden derlenmedi |
| `target/debug/xtask fixtures` | W | 0 | 24 parser-kernel fixture mevcut golden ile geçti |
| `target/debug/xtask rust-go-fixtures` | W | 0 | 8 fixture; complete ve partial örnekler |
| `target/debug/xtask js-ts-fixtures` | W | 0 | 11 fixture; JS/JSX/TS/TSX, malformed/Unicode |
| `target/debug/xtask csharp-java-fixtures` | W | 0 | 11 fixture; source-only ve negatives |
| `target/debug/xtask dart-fixtures` | W | 0 | 8 fixture; modern/partial/UTF-8-CRLF |
| `target/debug/xtask phase14-real-corpus` | W | 0 | 5 repository source dosyasında etiketli declarations; kapsamlı tüm-repo doğrulaması değil |
| `target/release/xtask grammar-check`, `fixtures`, `rust-go-fixtures`, `js-ts-fixtures`, `csharp-java-fixtures`, `dart-fixtures` | W | her biri 0 | Önce release xtask ile de geçti; yeni build değil |
| `target/release/xtask phase14-real-corpus` | W | 2 | Eski release xtask komutu tanımıyor; usage. Debug binary'deki kapı ayrıca geçti; uygulama kusuru sayılmadı |
| `PYTHONDONTWRITEBYTECODE=1 python3 C/scripts/phase15_package_smoke.py W/dist/codeatlas-0.1.0-aarch64-apple-darwin.tar.gz --output T/package-smoke.json` | W | 0 | Çıkarılmış paket, empty PATH, 7 dil, modern/legacy, corrupt-index discovery, EOF, geçici config round-trip geçti |

Paket SHA-256:

```text
57e64086ba5fb066f0fd79f1a2aeda71080e7cb68e8a7c8a7ca023327485c427
```

İç/dış checksum doğrulaması geçti. Smoke `--codex` olmadan çalıştırıldı;
`installed_codex_config_parse=not run`, `model_backed_codex_session=not run`.
Recorded package follower etiketi için C bölümündeki coverage uyarısı geçerli.
Paket yeniden oluşturulmadı; workflow dispatch, signing/notarization veya yayın yok.

### Bağımsız reproduction komutları

Komut prefix'i: `PYTHONDONTWRITEBYTECODE=1 python3 -u T/reproduce.py`;
çalışma dizini W. Script B binary'sini çalıştırıyor; bütün veri T altında.
Exit 0 aşağıdaki hatalı davranışın beklenip gözlendiği anlamına gelebilir;
uygulama acceptance test'inin geçtiği anlamına gelmez.

| Script argümanları / durum | Exit | Gözlenen sonuç |
|---|---:|---|
| `cycle` | harness 0; child timeout sonrası kill | 1,5 s sonunda resolving ve eşit-range cycle; CR-001 |
| `invalid` | 0; index child 0 / 0 | Mevcut invalid UTF-8 dosyası deletion ve completed; CR-002 |
| `go_package` | 0 | Farklı dizinler arasında yanlış kesin Go edge; CR-004 |
| `dart_hide rust_import_scope` | 0 | Dart show ve scope-dışı Rust use yanlış resolve; CR-009 / CR-005 |
| `ecma_exports` | 0 | Dört provider'da export const arrow unresolved; CR-010 |
| `member_references` | 0 | Beş dilde property→local yanlış kesin ilişki; CR-003 |
| `fingerprint_reparse` | 0 | İki kez parsed=1/reused=0; eski version/hash kaldı; CR-006 |
| `generations` | 0 | 5 düzenleme → 1 active + 4 superseded, 5 versions; CR-008 |
| `pagination` | 0 | 4.000 expected/4.000 unique, 68 sayfa; küçük pozitif kontrol |
| `pagination_large` | 0 | 11.000 expected/10.000 unique, 170 sayfa, terminal truncated=false; CR-011 |
| `wire_budget` | 0 | 72.732-byte tool frame, 70.747-byte prompt frame; CR-012 |
| `config_safety` | 0; malformed child 1 | Canary stderr'de, geçici valid apply/backup çalışıyor; CR-013 |
| `consistency` | 0 | Aynı logical rows; repeat reused=2; export edit'te unchanged importer reused=1 ama graph yeniden çözülüyor; full=incremental; rename/delete eski üyeliği temizliyor |
| `unicode_crlf` | 0 | Rust `café[7,12)`, `emoji[21,26)`, `later[47,52)` tam source byte dilimlerine eşit |
| `follower` | 0 | İki gerçek storage açılışı: owner/read_only_follower; iki query başarılı, follower index `WRITER_BUSY` |
| `boundaries` | 0 | 16 opt-in tool; fresh read; 5 path reddi; literal FTS-shaped query; memory revision conflict; stale read CONTENT_CHANGED; dış-root resource reddi |

Harness geliştirme sırasında başarısız denemeler saklanarak değerlendirildi:
ilk SQL column isimleri yanlıştı (exit 1); follower testinde zorunlu `mode`
eksikliği nedeniyle schema error'ı JSON envelope sanıldı (exit 1); bir rerun'da
zaten mevcut test symlink'i tekrar oluşturuldu (exit 1). Bunlar uygulama bug'ı
değil. Temp harness düzeltildi, ilgili testler yeniden exit 0 verdi. Ham modern
prompt denemesinde `_meta` eksikliği kontrollü protokol hatası verdi; ölçülen
70.747-byte kanıt geçerli metadata ile yeniden alınmıştır.

### Kullanıcının kritik senaryolarının kapsanması

| Senaryo | Bu turun kanıtı |
|---|---|
| İki kez index aynı logical sonuç | İzole JS örneğinde geçti; tüm dil/fixture kombinasyonları değil |
| Incremental ile temiz/full eşitliği | Küçük export-edit örneğinde logical symbol+call tuples eşit; oracle ayrıca beklenen hedefi kontrol etti |
| Delete ve rename/move | Aktif membership/symbol path temizliği geçti; directory/branch değişimi ve watcher-event varyantları tekrar edilmedi |
| Export/import invalidation | Source'u değişmeyen importer reused; yeni unresolved graph beklendiği gibi; arrow-export bug'ı ayrı bulundu |
| Aynı isim/scope sınırı | CR-003–005'te bağımsız negatif örnekler başarısız; bütün overload/generic matrisi tamamlanmadı |
| Syntax error / encoding | Mevcut partial fixture'lar geçti; invalid UTF-8 aktivasyonu CR-002; yeni bütün-iş hata izolasyon testi yok |
| İndeksleme sırasında snapshot | Transaction/generation source yolu incelendi; güncel parallel testler çalıştırılamadı |
| Cancellation/crash sonrası görünür veri | Source guards incelendi; CR-001 loop tehlikesi doğrulandı; geniş yeni crash/fault matrix yok |
| Unicode/CRLF | Mevcut dil fixture'ları + bağımsız Rust byte-slice oracle geçti; bütün platform line/column matrisi değil |
| Dış-root/symlink/ignore | Kontrollü macOS testinde reddedildi; Windows junction/drive/UNC test edilmedi |
| Büyük/bozuk MCP | Modern büyük prompt/ID budget bug'ı bulundu; crash görülmedi; malformed/schema rejection örnekleri kontrollü |

## E. Mimari ve kapsam özeti

### Gerçek uygulama ve bağımlılık sınırları

Uygulama artık yalnız plan/kit değil; gerçek CLI, indeks, SQLite, resolver ve
MCP handler'ları var. Ancak README “ready” durumları yukarıdaki hata örneklerine
karşı genel doğruluk güvencesi sayılamaz.

| Bileşen | İncelenen gerçek sorumluluk |
|---|---|
| `ca-core` | Repository/relative path/ID, byte ranges, cancellation ve typed domain modeller; rmcp/SQLite/Tree-sitter bağımlılığı yok |
| `ca-engine` | `SourceReader`/`FileScanner`, IndexService ve ports, full-generation resolution, RetrievalService, memory use case |
| `ca-languages` | LanguageRegistry, worker-local ParserWorker, compiled embedded queries, dil adaptörleri ve syntax observations |
| `ca-storage` | rusqlite, schema v6, OS writer lock, dedicated writer actor, read snapshots, FTS5, atomic generations, jobs/memories |
| `ca-mcp` | Resmî rmcp tool/prompt/resources kayıtları, typed request schemas, envelope/result bütçesi, SDK stdio lifecycle |
| `ca-cli` | Clap giriş noktası, parser/storage/retrieval adapters, retained index worker, owner-only watcher, Codex config entegrasyonu |
| `xtask`, fixtures/tests/fuzz/scripts | Gate runner, extraction oracle/goldens, CLI/lifecycle/storage tests, fuzz ve paket/ölçüm yardımcıları |

Manifest/lock ile doğrulanan ana pins: uygulama 0.1.0, Rust 1.98.1, rmcp 3.4.0,
Tree-sitter 0.27.0, rusqlite 0.40.2 (`bundled-full`), toml_edit 0.25.15. Uygulama
storage'ı SQLite safety floor/FTS5/WAL kontrol ediyor; kayıtlı/paketli runtime
SQLite 3.53.2. Varsayılan feature setinde zorunlu HTTP, Redis, Neo4j, embedding,
model/SDK indirmesi veya daemon bulunmadı. Native grammar'lar uygulama içine linked;
bu bir memory-safety sandbox değildir.

Transport doğrudan SDK stdio. Modern 2026-07-28 `server/discover` + request metadata
ve legacy 2025-11-25 initialize/initialized ayrı smoke edildi. MCP Tasks
advertise edilmiyor. Varsayılan 14 tool; trusted `--memory-write` ile 16.
Prompt'lar şablon; uygulama içeride LLM çalıştırmıyor. Remote MCP client'ın
retrieved code'u model servisine iletebilmesi local runtime'dan ayrı gizlilik konusu.

### Dosyadan MCP yanıtına izlenen yol

```text
ca-cli::main / run(Serve)
  → trusted startup RepositoryRoot
  → McpBackend::with_watch / ca_mcp::serve_stdio (rmcp)
  → typed tools/call handler → spawn_blocking backend dispatch
  → McpBackend::index_repository / IndexService::prepare
  → retained index thread + index_gate → IndexService::run_prepared
  → FileScanner + authorized no-follow SourceReader, ignore + content hash
  → ParserFactory / LanguageExtractionWorker / ParserWorker
  → embedded .scm captures + adapters → IndexedFile
  → StorageIndexAdapter → writer actor stage_files / stage_file
  → full-generation resolve_generation → generation-scoped graph staging
  → activate_generation: membership/graph/active pointer transaction
  → ReadSnapshot (generation pinned) → StorageRetrievalAdapter / RetrievalService
  → hash-validated fresh source / bounded search, graph, context
  → ApplicationEnvelope → CallToolResult → rmcp JSON-RPC stdout frame
```

Discovery ve cached status DB/scan başlamadan çalışıyor. Storage lazy açılıyor;
follower query-only. Source read policy ignore/containment'i retrieval sırasında
yeniden uyguluyor. Parametreli SQL ve literal FTS expression üretimi var.
Traversal BFS cycle/node/edge/depth policy kullanıyor. Index writer ve memory
writes aynı serialized storage sınırında; memory authored/untrusted ve optimistic
revision'lı. Memories indeks nesillerinden bağımsız. Watcher event'leri bounded
hint/coalescing; periodic scan/hash authoritative, event'ten doğrudan membership
silinmiyor. CR-001, 002, 006–008 bu tasarımın somut ihlal noktalarıdır.

### Dil bazında implementasyon ve kanıt

Her provider'ın ayrı symbols/imports/references/calls query asset'i ve gerçek
adapter yolu görüldü; yalnız `set_language` başarısı “destek” sayılmadı. Syntax
gözlemi ile compiler-resolved dispatch ayrımı korunması gereken sözleşmedir.

| Dil / pin | Görülen gerçek çıkarım ve sınır | Bu tur kanıtı / kusur |
|---|---|---|
| Rust / grammar 0.24.2 | Functions/types/impl/locals/parameters, scope, grouped use/alias, reference/call/macro, cfg; macro expansion/trait dispatch yok | 8 Rust-Go ortak fixture + temel kernel; CR-001/003/005; UTF-8/CRLF oracle |
| Go / 0.25.0 | Types/functions/methods/receiver, local/parameter, package/import, selector/calls, build tags; interface/build selection semantiği yok | Rust-Go suite; CR-003/004 |
| C# / 0.23.5 | Classes/records/methods/properties/events, scopes/usings, attributes, invocations/new/preprocessor; partial groups/overload/dispatch limitations | 11 C#-Java ortak fixture; CR-003 |
| Java / 0.23.5 | Type/method/field/parameter, package/import/static import, scopes/calls/annotations; method reference call değildir, classpath/compiler yok | C#-Java suite; CR-003 |
| Dart / 0.2.0 | Library/import/export/part, classes/mixins/extensions/constructors/getter-setter/patterns, scopes/ref/calls; analyzer/package-config/parts/dispatch limits | 8 Dart fixture, modern/partial/UTF-8-CRLF; CR-003/009; grammar'ın non-ASCII identifier limiti belgelenmiş, bug sayılmadı |
| JavaScript / 0.25.0 | Functions/classes/function-binding/scopes, ESM/CommonJS/static module observations, refs/calls; dynamic package exports/runtime yok | 11 JS-TS ortak fixture; arrow-export CR-010 |
| JSX / JS grammar 0.25.0 | JSX component/member syntax ayrı query/adapter role; HTML intrinsic adları component binding'i sayılmıyor | JSX fixture ve ayrı export reproduction; CR-010 |
| TypeScript / 0.23.2 | Type/value roles, interface/type alias/generics, function binding, imports/exports/scopes; tsconfig/package alias resolution yok | JS-TS suite, malformed/Unicode; CR-010 |
| TSX / ayrı TSX grammar 0.23.2 | TS generic/type/value ve JSX rolleri; ayrı provider/grammar fingerprint | TSX fixture ve ayrı export reproduction; CR-010 |

### Derinlik/kapsam kaydı

Yaklaşık 29 bin Rust satırı (inline testler dahil) envantere alındı. “Tüm
uygulama kapsamı”, her satır/test/fixture'ın eşit derinlikle incelendiği anlamına
gelmez. Kritik uçtan uca yollar önceliklendirildi.

| Alan | İnceleme derinliği |
|---|---|
| Repository scanner/reader | Production containment, no-follow, ignore, hash ve scan outcomes derin; bütün inline testler tek tek değil |
| Indexing | Prepare/run/scan→stage→resolve→activate, worker/result queues, cancellation/failure yolları derin |
| Resolution | Lexical/import/same-module/export, scope helpers, certainty/edge construction derin; Rust module heuristics ve her dil özel dalı aynı kapsamda dinamik sınanmadı |
| Retrieval | Search/cursor/budgets, symbol/read freshness, references/calls/map/impact/context ana yollar derin; bütün property/test kombinasyonları çalıştırılmadı |
| Languages | Parser/query lifecycle, common extraction, 7 dil+JSX/TSX adapter ana yolları ve query assets derin; her modern construct/helper edge case'i değil |
| Storage | Immutable version key, generation staging/activation, snapshots/FTS, FK/migrations, writer/lock/recovery/GC/memory ana yollar derin; tüm command dispatch/inline test satırları değil |
| MCP/CLI | Kayıt/schema/dispatch, stdio/shutdown, backend index/retrieval, config integration ana yolları derin; tüm CLI option çapraz kombinasyonları değil |
| Watcher/memory | Production hint/coalescing/periodic/owner ve revision/freshness yolları okundu; yeni uzun süreli watcher/concurrency matrisi çalışmadı |
| Tests/fixtures | Suites/oracle/negative assertions ve kritik test bölümleri tarandı; mevcut binary extraction kapıları çalıştı; kaynak unit/integration testleri derlenemedi |
| Fuzz/bench/CI | Target/script/komutlar incelendi; yeni fuzz/bench/advisory matrix veya hosted CI koşusu çalıştırılmadı |
| Docs/pins/release | README, mimari/dil/storage/MCP/privacy/dependency kararları, ilgili ADR'ler, Phase 15 diff/report/package scripts incelendi; tüm tarihsel rapor/loglar yeniden doğrulanmadı |
| External/native kaynaklar | Cargo lock/pins ve grammar registry/query uyumu incelendi; bütün third-party C/FFI implementasyonlarının memory-safety audit'i yapılmadı |

Production yolunda yeni bir TODO/unimplemented/stub-success kusuru doğrulanmadı.
Test panic/unwrap veya build-time process çağrıları production input bug'ı diye
listelenmedi. Manifest/lock/CI incelendi ama mevcut Rust binary'lerini güncel
source test build'i yerine koymak özellikle reddedildi.

## F. Düzeltme planı

Bu plan öneridir; bu tur uygulanmadı. Yeni servis, compiler/LSP sidecar, vector
DB veya dış LLM gerektirmez. Önce toolchain'i kullanıcı onayıyla erişilebilir
kılarak güncel source baseline testleri çalıştırılmalı; bu review'in izinleri
böyle bir kurulum için genişletilmedi.

| Sıra | CR / modül | Önerilen iş | Test edilebilir kabul kriteri |
|---|---|---|---|
| 1 | CR-001 / resolver + scopes | Acyclic parent policy, cycle/depth/cancel guard | LF'siz fixture resolve tamamlanır; bounded chain; cancel/EOF sonlu; belirsiz target unresolved |
| 2 | CR-002 / scanner + index pipeline | Failed read ile deletion/exclusion ayrımı | Invalid UTF-8/NUL/oversize full/targeted aktivasyonu durdurur; healthy generation aynı; gerçek deletion çalışır |
| 3 | CR-006 / storage identity + migrations | Tam analiz fingerprint key'i, immutable new versions | Grammar-only değişimde yeni provenance; ikinci run reuse; eski snapshot/facts ve notes değişmez; migration rollback güvenli |
| 4 | CR-003 / adapters + resolver | Member property'sini lexical bare-name'den ayır | Beş dilde property asla local'e kesin bağlanmaz; bare-local pozitifler, type/value rolleri korunur |
| 5 | CR-004, CR-005 / resolver | Go directory-package identity; scope-aware imports | Farklı Go directory negatif; aynı directory pozitif; Rust sibling use sızmaz; alias shadowing doğru; CR-001 guard kullanılır |
| 6 | CR-009, CR-010 / language extraction + binding | Dart combinators; ESM export wrapper/map | Show/hide yasak adlar unresolved; allowed resolve; dört ECMA provider export const arrow import'unda doğru hedef |
| 7 | CR-007 / index concurrency | Producer/consumer interleave, bounded result channel | Buffer high-water policy'yi aşmaz; slow DB/worker failure/cancel deadlock yok; atomic activation korunur; ölçülen RSS kaydedilir |
| 8 | CR-008 / storage GC | Cursor-window/quota retention, FK-safe GC | Yüzlerce index/edit'te retained counts bounded; active/previous ve memories korunur; expired cursor STALE_CURSOR |
| 9 | CR-011 / retrieval storage + cursors | Limit öncesi keyset veya honest cap exhaustion | >10k oracle sonuçları tam gezilebilir veya açık incomplete; terminal false-complete yok; dedup/filter/generation stable |
| 10 | CR-012 / MCP schemas/SDK transport | Prompt/input ve gerçek envelope byte policy | Modern+legacy her başarı/hata emitted frame ≤65.536; Unicode/escaping/ID cases; controlled overlimit rejection |
| 11 | CR-013 / config errors | Source-text'siz sanitize edilmiş diagnostics | Malformed canary stdout/stderr'de yok; config original bytes aynı; ownership/round-trip pozitifler geçer |
| 12 | Bütün CR'ler / doğrulama | Güncel locked/offline build, negatifler, native macOS smoke | fmt/check/clippy/tests + tüm xtask kapıları güncel source'da exit 0; yeni negatifler golden auto-accept olmadan; gerçek follower ve cursor/budget assertions |

CR-001–005 doğruluk/yayın engelleri kapanmadan kapsamı genişletmek yerine bu küçük
negatif oracle'lar ana regression suite'e eklenmeli. CR-006 migration işi, daha
sonraki extractor fingerprint yükseltmelerinin eski index'e doğru uygulanmasının
önkoşuludur. Her grammar/adapter/resolver değişiminde query/extractor/resolver
versiyon politikası bilinçli izlenmeli; notlar yeniden indeksleme ile silinmemeli.

Son doğrulama macOS ARM64 üzerinde yeni binary ve extracted package ile yapılmalı.
Kayıtlı eski Phase 14/15 ölçümleri yeni bug-fix binary'sinin ölçümü değildir.
Linux/Windows sonraki native qualification işidir; macOS fix'leri o platformlarda
geçmiş sayılmaz. Model-backed Codex smoke, kaynak iletimine ayrıca yetki verilmeden
başlatılmamalı; review yalnız yerel subprocess clients kullandı.

Stil/refactor listesi oluşturulmadı; bulgular somut işlev/doğruluk/sınır ihlalleridir.
Kaynak, test, manifest, lockfile ve config üzerinde değişiklik yapılmadı;
branch/HEAD ve kullanıcının mevcut Git değişiklikleri korundu.
Kalıcı çalışma ağacı çıktısı yalnız bu rapordur; izole reproduction/build-test
dosyaları geçici dizindedir. Düzeltme, commit, push, publish veya gerçek Codex
kurulumu/removal uygulanmadı.
