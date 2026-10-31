# Taskbaroque

Windows 11 için hafif, özel görev çubuğu ve masaüstü widget'ları. Rust ile doğrudan Win32 ve Direct2D kullanılarak yazıldı. Tarayıcı motoru yok, tek bir küçük `.exe`.

## Özellikler

- **Özel görev çubuğu.** Her ekranda bir tane olur. İçinde şunlar var:
  - Başlat düğmesi
  - Açık pencereler. Tıklayınca öne gelir ya da küçülür, orta tıkla kapanır.
  - Çalan medya
  - Hava durumu
  - CPU ve RAM
  - Saat. Tıklayınca takvim ve bildirimler açılır.
- **Masaüstü düzeni bozulmaz.** Windows görev çubuğu yalnızca görünmez yapılır. Otomatik gizleme ayarına dokunulmaz. Çubuk Windows'un ayırdığı alana oturur, bu yüzden simgeler ve pencereler kaymaz.
- **Tam ekran uygulamalar.** Oyun veya video tam ekrandayken çubuk o ekranda gizlenir.
- **Widget'lar:**
  - Saat ve takvim
  - Hava durumu (Open-Meteo, API anahtarı gerekmez)
  - Medya (kapak resmi ve kontroller)
  - Sistem monitörü (CPU, RAM, disk, ağ ve 60 saniyelik grafik)
- **Widget yönetimi.** Sağ tık menüsünden açılıp kapatılabilir ve yeni widget eklenebilir. Sürükleyerek taşınabilir, başka ekrana da taşınabilir. Konumlar otomatik kaydedilir.
- **Temalar.** 14 hazır tema var: Gece mavisi, Aydınlık, Aurora, Buzlu cam, Neon, Orman, Fırçalı metal, Retro piksel, Pastel, Minimal, Yuvarlak dock, Modüler, Gün batımı ve Siyah (AMOLED). Temalar renk geçişi, renkli ışıklar, parlayan kenarlık, simge kutucukları, farklı açık uygulama işaretleri ve ayrı panelli (modüler) düzen kullanabilir. Sağ tık menüsündeki **Tema** bölümünden tek tıkla değişir. `[themes.isim]` bölümüyle kendi temanı tanımlayabilirsin, eksik renkler `base` ile seçtiğin temadan alınır.
- **Tema ve ayarlar.** `%APPDATA%\Taskbaroque\config.toml` dosyasında tutulur. Dosyayı kaydettiğinde değişiklikler hemen uygulanır.

## Derleme

```
cargo build --release
target\release\taskbaroque.exe
```

## Güvenlik

- Menüdeki **Çıkış** seçeneği Windows görev çubuğunu geri getirir.
- Uygulama çöker veya zorla kapatılırsa `taskbaroque.exe --restore` komutunu çalıştır.
