$ca-phase-driver $ca-treesitter-language $ca-mcp-contract

Bu klasördeki CodeAtlas MCP projesini Rust ile geliştiriyoruz.
Önce AGENTS.md, README.tr.md ve docs/PROJECT_STATE.md dosyalarını oku.
Ardından prompts/00-compatibility-spike.md dosyasındaki görevi uygula.

Yalnızca Aşama 00 üzerinde çalış. Sadece plan sunma: izin verilen çalışma
alanında uyumluluk denemelerini, testleri ve kanıt raporunu oluştur.
Mevcut dosyaları ve kullanıcı değişikliklerini koru. Eski codebase-memory-mcp
kurulumunu değiştirme; ~/.codex/config.toml dosyasına müdahale etme.

Bütün yedi dil ve JSX/TSX için aynı Tree-sitter runtime'ıyla grammar yükleme ve
küçük parse testlerini doğrula. Dart paketinin kaynağını ayrıca kontrol et.
Modern ve eski MCP yaşam döngülerini seçilen yayımlanmış rmcp sürümüyle test et.
Dokümantasyonda görünen sürümü derlenmiş/test edilmiş sürüm gibi raporlama.

Kabul ölçütleri geçmeden Aşama 01'e geçme. Eksik araç veya yetki varsa güvenli
bağımsız işleri tamamla; geçmeyen kontrolleri ve gerçek engeli açıkça kaydet.
Sonuçları docs/reports/00-compatibility-spike.md dosyasına yaz ve
PROJECT_STATE.md içindeki sonraki adımı güncelle. Commit, push, publish yapma.
Bana Türkçe olarak değişiklikleri, çalıştırılan komutların sonuçlarını ve
kalan sınırlamaları bildir.
