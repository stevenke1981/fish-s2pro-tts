# 🐟 Fish Audio S2.1 Pro TTS Studio

> 一款以 **Rust 原生 GUI (`egui` + `eframe`)** 打造的極速、輕量級語音合成桌面應用程式。直接整合 [OpenRouter Fish Audio S2.1 Pro Free:free](https://openrouter.ai/fish-audio/s2.1-pro-free:free#playground) 模型，支援口氣情緒控制、角色音色設定、台詞腳本庫與內建音訊播放與匯出。

---

## 🌟 核心特色

1. **純 Rust 原生 GUI**：
   - 使用 `egui 0.31` 與 `eframe` 構建，啟動即時（< 0.1 秒），記憶體佔用極低。
   - 預載微軟正黑體（Microsoft JhengHei）字型渲染，繁簡中文字元、標點符號與符號完整清晰呈現，杜絕缺字亂碼（豆腐塊）。
   - 支援深色模式（Dark）與淺色模式（Light）一鍵切換。

2. **完整對接 OpenRouter Fish Audio S2.1 Pro**：
   - 預設免費模型：`fish-audio/s2.1-pro-free:free`
   - 支援切換 `fish-audio/s2.1-pro`（生產付費級）、`fish-audio/s2-pro`、`fish-audio/s1` 或自訂模型。
   - 提供 API Key 掩碼切換、連線有效性測試（顯示可用額度/免費層標籤）與安全本機儲存（`fish_tts_config.json`）。

3. **強大的角色與聲線設定（角色）**：
   - 內建 8 種預設角色聲線：
     - **Fish 原生預設聲線**（官方標準）
     - **溫柔知性御姐**
     - **活力元氣少女**
     - **沉穩成熟青年**
     - **熱血陽光少年**
     - **紀錄片旁白解說**
     - **智慧科技助手**
     - **傲嬌大小姐**
   - 支援填入任意自訂 Fish Audio Voice ID / reference_id。

4. **直覺的情緒與口氣控制（口氣等）**：
   - Fish Audio S2.1 原生支援以方括號 `[tag]` 進行行內自然語言情緒轉折。
   - 快速標籤列（分類篩選）：
     - **基礎情緒**：`[happy]`、`[sad]`、`[angry]`、`[excited]`、`[calm]`、`[whispering]`、`[serious]`、`[sarcastic]`、`[crying]`、`[fearful]`、`[shy]`、`[proud]`
     - **副語言動作**：`[sigh]` 嘆氣、`[chuckle]` 偷笑、`[gasp]` 倒抽氣、`[pant]` 喘氣、`[throat-clearing]` 清喉嚨、`[laughing]` 大笑
     - **說話風格**：`[溫柔細語]`、`[廣播播音腔]`、`[說故事口吻]`、`[冷靜理性]`、`[充滿決心]`
     - **自訂自由口氣**：可自定義任何文字描寫（例如 `[像在耳邊說悄悄話]`），一鍵新增並保存。

5. **內建高保真音訊播放器與管理**：
   - 基於 `rodio` 的低延遲音訊串流解碼。
   - 支援播放、暫停、停止、重播、即時進度條拖曳與音量控制。
   - 自動儲存生成音檔至 `outputs/` 目錄，支援歷史紀錄回放。
   - 一鍵「另存音檔」（支援 MP3 / WAV 格式匯出）與「開啟輸出資料夾」。

6. **生成的語音檔案管理頁面 (File Management Page)**：
   - `outputs/` 資料夾全功能檔案總管，自動提取檔名、對應配音角色、台詞摘要、時長、檔案大小與建立時間。
   - 獨立頂部音訊播放器控制列：即時波形進度條、拖曳跳轉、音量調節、重播與播放狀態標記。
   - 列表行內單一播放/暫停/繼續切換、安全更名（防止副檔名遺失或重複）、支援 MP3 轉標準 WAV 匯出、單檔永久刪除。
   - 支援搜尋與篩選：關鍵字模糊搜尋、MP3/WAV 格式分類篩選、多維度雙向排序（時間、大小、時長、名稱）。
   - 批次管理能力：全選/反選/批次刪除與「清理 7 天前舊檔」，具備防誤刪確認對話框。

7. **多角色語音生成管理頁面 (Multi-Character Speech Generation)**：
   - 劇組演員陣容（Cast）管理：自訂多位發話者（Speaker 0, 1, 2...），可指定個別角色聲線、Voice ID、預設語氣與專屬頭像色彩標籤。
   - 逐行對白劇本編輯器：彈性指定發話者、台詞文字、該句專屬情緒口氣（支援一鍵還原預設語氣）與句間停頓時長（0~5000ms）。
   - 支援行間上下移動排序、單句獨立試聽預覽（附 spinner 載入反饋）。
   - 雙重合成引擎模式：
     - **原生多角色 Prompt 語法模式**：利用 `<|speaker:X|>` 與情緒標籤單次請求 Fish Audio 模型進行自然多角色對白合成。
     - **循序合成與無縫拼接模式**：逐句請求並自動插入精確的毫秒級靜音（支援 WAV PCM 與 MP3 格式），即時回報合成進度條與步數。
   - 支援純文字劇本結構化匯出（【登場角色配置】與【劇本對白內容】）及合成完整對白音訊匯出。

---

## 🚀 快速開始

### 執行 Release 執行檔

編譯完成的免安裝單一執行檔位於：
```bash
target\release\fish-s2pro-tts.exe
```
直接雙擊即可啟動原生視窗！

### 本機原始碼編譯

若欲自行編譯或除錯：
```bash
# 檢查相依與編譯
cargo build --release

# 執行測試套件
cargo test --all-targets

# 程式碼風格與靜態檢查
cargo clippy --all-targets

# 開發模式執行
cargo run
```

---

## 🛠️ 專案架構

```
D:/fish-s2pro-tts/
├── Cargo.toml                  # 專案相依與清單
├── fish_tts_config.json        # 本機配置與金鑰安全儲存
├── outputs/                    # 每次合成音檔自動快取目錄
├── src/
│   ├── main.rs                 # 桌面視窗入口與原生執行器
│   ├── lib.rs                  # 函式庫模組匯出
│   ├── app.rs                  # egui 原生 UI 介面、分頁路由與非同步通訊
│   ├── api.rs                  # OpenRouter TTS 與 Key Auth API 客戶端
│   ├── audio.rs                # rodio 音訊播放器、PCM/WAV 轉碼與時長估算
│   ├── config.rs               # 設定檔讀寫與序列化
│   ├── file_manager.rs         # 語音檔案總管頁面、批次管理、排序篩選與專用播放列
│   ├── fonts.rs                # Windows 中文字型動態綁定
│   ├── models.rs               # 角色預設、情緒標籤庫與示範腳本
│   └── multi_speech.rs         # 多角色對白管理、演員陣容、逐句編輯與靜音拼接引擎
└── tests/
    └── integration_tests.rs    # 端到端與邊界情況自動化測試 (13 項測試)
```

---

## 🔑 OpenRouter API Key 取得方式

1. 前往 [OpenRouter 官方網站](https://openrouter.ai/) 註冊或登入。
2. 前往 [Keys 頁面](https://openrouter.ai/keys) 建立一個新的 API Key。
3. 啟動本 App，在左側欄貼上 API Key（格式如 `sk-or-v1-...`）。
4. 點擊「測試連線」驗證金鑰狀態後，即可暢享語音合成！
