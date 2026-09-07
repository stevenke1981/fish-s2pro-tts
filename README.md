# 🐟 Fish 語音工作室

[![Rust validation](https://github.com/stevenke1981/fish-s2pro-tts/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/stevenke1981/fish-s2pro-tts/actions/workflows/rust.yml)

**用繁體中文寫台詞、設計角色、生成語音，再透過多軌時間軸完成混音。**

Fish Audio S2.1 Pro TTS Studio 是以 Rust、egui／eframe 建立的原生桌面 App，透過 OpenRouter 呼叫語音模型，整合單人配音、多角色劇本、故事導演建議與時間軸編輯。

[快速開始](#快速開始) · [操作流程](#操作流程) · [時間軸](#時間軸與混音) · [已知限制](#已知限制) · [測試與開發](#測試與開發)

> **目前為開發中版本。** 時間軸尚無工程儲存／重新開啟功能；關閉前請另存台詞並匯出 WAV。WAV 是最終音訊，不能還原可編輯的軌道與片段。

## 可以做什麼？

| 工作區 | 功能 |
| --- | --- |
| 單人配音 | 大型台詞編輯器、UTF-8 文字匯入／另存、範例台詞、一次替換／清空復原、角色聲線、語速與輸出格式 |
| 故事導演 | 有聲書、武俠、懸疑、童話、睡前故事等體裁；情緒與導演標籤；規則式建議與靜態劇本檢查 |
| 多角色劇本 | Cast 設定、個別 Voice ID／語氣／語速、逐行對白、句間停頓、單句試聽與整份生成 |
| 時間軸 | 角色分軌草稿、音檔匯入、波形、移動、修剪、分割、音量、Mute／Solo 與 WAV 混音 |
| 作品庫 | 搜尋、格式篩選、排序、播放、更名、匯出、刪除與傳送至時間軸 |

介面支援深色／淺色主題。進階模型設定、語氣標籤與歷史預設收合，主要流程聚焦於 **寫台詞 → 選聲音 → 生成與試聽**。

### 時間軸與單人配音介面更新

- 角色聲線與自訂 Voice ID 位於單人配音的台詞上方。
- 台詞框上方提供常駐快捷標籤：平靜、開心、悲傷、低語、激動、停頓、嘆氣、清唱；可選擇插入開頭或追加末尾。
- 時間軸採左側固定軌道控制、右側水平時間尺與波形片段。拖曳片段中央可調整起始時間或跨軌移動，兩側把手可修剪；按住 Shift 拖曳以 0.1 秒對齊。上下捲動時標頭與軌道同步。
- 清唱快捷鍵插入 `[singing] [a cappella, unaccompanied solo voice]`，建議放在歌詞開頭。這是實驗性語氣提示，無法指定樂譜、旋律或保證音準與無伴奏效果，並非獨立歌聲合成引擎。官方列有 `[singing]` 標籤：[Fish Audio S2 Pro 模型說明](https://huggingface.co/fishaudio/s2-pro/blob/main/README.md)。目前未執行線上清唱生成驗證。

## 快速開始

### 自行編譯的環境需求

- Git。
- Rust stable 工具鏈與 Cargo（專案使用 Rust 2024 edition，請保持工具鏈更新）。
- Windows 編譯環境需具備 MSVC C++ 建置工具與 Windows SDK。
- 可連線至 OpenRouter 的網路，以及你的 API Key。
- 播放需要可用的音訊輸出裝置；無裝置仍可保留生成結果並另存。

### 直接下載執行檔（不需 Rust）

前往 [GitHub Releases](https://github.com/stevenke1981/fish-s2pro-tts/releases)，或開啟 [最新開發版 Preview](https://github.com/stevenke1981/fish-s2pro-tts/releases/tag/preview)。

| 平台 | 下載檔案 | 啟動方式 |
| --- | --- | --- |
| Windows x86_64 | `fish-s2pro-tts-windows-x86_64.zip` | 完整解壓縮到可寫入資料夾，雙擊 `start.cmd` |
| Linux x86_64 | `fish-s2pro-tts-linux-x86_64.tar.gz` | 解壓縮後，在桌面環境執行 `./start.sh` |

啟動腳本會切換至程式所在目錄，方便保留設定與輸出。每份套件附有 `BUILD.txt`（來源提交）與獨立 SHA-256 校驗檔。Linux 版本以 Ubuntu 22.04 建置，需相容的 glibc、OpenSSL、ALSA、udev 與圖形環境；不是靜態連結的通用 Linux 執行檔。

Preview 會隨成功的主線建置更新。套件目前沒有程式碼簽章；Windows 若提示缺少 VC++ 執行階段，需安裝對應的 x64 執行階段。下載位置首次出現檔案的時間取決於建置是否完成。

以下為開發者自行編譯的方式。

### Windows／PowerShell

```powershell
git clone https://github.com/stevenke1981/fish-s2pro-tts.git
cd fish-s2pro-tts
rustup update stable
cargo build --release --locked
.\target\release\fish-s2pro-tts.exe
```

建議從專案目錄啟動，讓設定與輸出檔案位置維持一致。

### Linux

CI 使用 Ubuntu，原生建置相依套件如下：

```bash
sudo apt-get update
sudo apt-get install -y libasound2-dev libudev-dev pkg-config libssl-dev
git clone https://github.com/stevenke1981/fish-s2pro-tts.git
cd fish-s2pro-tts
cargo build --release --locked
./target/release/fish-s2pro-tts
```

桌面執行另需可用的圖形與音訊環境。中文字型會依序嘗試系統候選字型；Linux 包含 Noto Sans CJK／文泉驛，Windows 優先使用微軟正黑體，macOS 包含 PingFang 候選路徑。實際顯示取決於字型是否存在，詳見 [fonts.rs](src/fonts.rs)。

### 更新已下載的專案

確認本機修改已妥善保存，再執行：

```powershell
git switch main
git pull --ff-only origin main
cargo build --release --locked
```

## 操作流程

### 1. 設定連線

在「單人配音」左側展開「連線設定 · OpenRouter」，填入從 [OpenRouter Keys](https://openrouter.ai/keys) 建立的 API Key，再按「測試連線」。

- 金鑰可隱藏顯示。
- 勾選「記住金鑰」會將金鑰以**明文**存入本機設定檔；不勾選時，儲存設定會清空磁碟上的金鑰欄位。
- 預設模型識別碼為 `fish-audio/s2.1-pro-free:free`。模型供應、費用、額度與可接受參數以 OpenRouter 實際回應為準，App 不保證免費模型持續可用。
- 單人配音可展開進階模型設定，選擇其他內建識別碼或自訂模型。多角色頁面有自己的生成設定；時間軸 TTS 目前使用程式內建的預設模型。

### 2. 寫台詞與選聲音

貼上文字，或匯入 UTF-8 的 `.txt`／`.md` 檔案，再選擇角色、語速及格式。

```text
[calm] 很久以前，山腳下有一間小小的茶館。
[whispering] 「你聽見了嗎？門外好像有人。」
[excited] 「是他回來了！」
```

標籤是傳給模型的演繹提示，效果需以實際試聽確認。角色聲線預設主要由提示詞組成；固定聲音可填入 Voice ID，但仍需確認模型與供應端是否接受。

- 「儲存台詞」可另存文字。
- 清空、匯入或套用範例後，可使用「復原替換／清空」取回上一次文字。
- 一次復原不是自動儲存或完整版本紀錄。

### 3. 生成、試聽與另存

確認模型、格式與語速後，按「產生語音」。

單人與整份多角色生成結果會嘗試寫入 `outputs/`。關閉自動播放時，最新生成音訊仍會載入播放器；儲存失敗時會顯示另存指引，不將不存在的音檔加入歷史。

單人生成結果可送至時間軸，並保留生成當時的台詞、角色與 Voice ID，避免與其他頁面播放的音訊混淆。

## 故事導演與多角色劇本

在台詞編輯器下展開「故事導演與劇本檢查」，選擇體裁並套用建議。

**自動導演是本機規則處理。** 它依體裁、引號與標點加入演繹建議，不會呼叫另一個 LLM 分析故事。靜態 QA 可提示過長段落、標籤堆疊、方括號閉合、過多驚嘆號與空台詞；不能代替情節校稿或音訊品質驗收。

多角色頁面提供兩種生成方式：

| 模式 | 行為 |
| --- | --- |
| 原生多角色 | 以 `<\|speaker:X\|>` 等語法組合劇本，單次請求合成 |
| 循序生成與拼接 | 逐句請求，再依句間停頓設定拼接，顯示處理進度 |

原生多角色模式不等同於逐角色套用每一個自訂 Voice ID。需要個別聲音設定時，請使用循序模式並確認供應端支援。需要檢查停頓與後續編輯時，可使用 WAV 流程。

## 時間軸與混音

| 匯入方式 | 放入時間軸的內容 |
| --- | --- |
| 單人配音「傳送至時間軸」 | 該次生成的完整音訊與來源台詞 |
| 多角色「將劇本匯入時間軸多軌」 | 依角色建立草稿，保留聲線、Voice ID、語氣與語速；需再逐句生成 |
| 多角色「匯入上次完整混音」 | 完整合成音訊，使用生成當時的劇本資料 |
| 作品庫／本機音檔匯入 | 已有的音訊檔案，可作為配音、BGM 或音效素材 |

整份混音沒有逐句時間碼，因此**不依字數比例強制切句，也不進行聲源分離**。需要精準分軌時，可先匯入草稿逐句生成，或依實際聲音位置手動分割。

- 片段支援拖曳、修剪、分割與速度／增益調整。
- 軌道支援音量、Mute 與 Solo。
- 草稿沒有可播放音訊；請先生成，或將草稿所在軌道靜音，再播放／匯出混音。
- 生成中的目的片段會受到保護；切換分頁後仍會處理時間軸工作完成訊息。
- 離開時間軸頁面會暫停時間軸播放。

快捷鍵在非文字編輯狀態下使用：

| 快捷鍵 | 動作 |
| --- | --- |
| Space | 播放／暫停 |
| S 或 Ctrl+B | 在播放頭位置分割 |
| Delete／Backspace | 刪除選取片段 |

## 音訊格式與資料保存

| 操作 | 支援範圍 |
| --- | --- |
| API 生成格式 | 可選 MP3／WAV，實際回應由供應端決定 |
| 已有 MP3 另存 | 保留 MP3，或解碼轉為 WAV |
| 已有 WAV 另存 | WAV；不支援 WAV 編碼為 MP3 |
| 時間軸混音 | 44.1 kHz、立體聲、16-bit PCM WAV |
| 時間軸工程檔 | 尚未實作 |

| 位置 | 內容 |
| --- | --- |
| `fish_tts_config.json` | 偏好設定與可選的明文 API Key；優先讀取工作目錄既有檔案，亦會嘗試執行檔旁的既有設定 |
| `outputs/` | 以目前工作目錄為基準的生成音檔與匯出混音 |
| `outputs/history.json` | 生成歷史索引，目前最多保留 50 筆；索引上限不等同於自動刪除音檔 |
| 使用者指定位置 | 另存的台詞與音訊 |

時間軸新生成片段主要保留於記憶體，不應視為已自動備份。搬移程式或更換啟動目錄前，請一併保存設定與輸出資料。

## 已知限制

- 尚無時間軸工程儲存／重新開啟、自動復原、完整 Undo／Redo。
- 尚無生成工作取消或持久化工作佇列；關閉視窗前請等待工作完成並保存結果。
- 沒有 MP3 編碼器；修改副檔名不能把 WAV 轉為 MP3。
- 自然語言導演標籤及 Voice ID 的效果、支援程度需依模型實測。
- 時間軸剪輯與混音是目前實作的功能，不代表具備完整商用影音剪輯軟體的能力。
- Windows／Linux CI 驗證建置與程式測試；macOS、真實語音生成、中文字型排版、拖曳體驗與實際音訊裝置仍需人工驗收。

## 常見問題

**中文顯示成方框？**  
確認系統有 [fonts.rs](src/fonts.rs) 列出的中文字型。App 不會自動下載字型。

**生成完成卻沒有聲音？**  
檢查自動播放設定、音量與系統輸出裝置。可以手動播放；沒有裝置時仍可另存音訊。

**混音提示有未生成草稿？**  
選取草稿並生成 TTS，或將該軌靜音。草稿不會以假音訊混入成品。

**API 回傳錯誤？**  
檢查金鑰、額度、模型識別碼與錯誤訊息。不要假設程式內建的模型清單代表即時可用狀態。

**設定或作品找不到？**  
確認啟動時的工作目錄。建議固定從專案目錄執行，並查看其中的 `outputs/`。

## 測試與開發

```bash
cargo test --locked
cargo build --locked
cargo clippy --locked --all-targets
cargo run --locked
```

[GitHub Actions](https://github.com/stevenke1981/fish-s2pro-tts/actions/workflows/rust.yml) 在 Windows／Ubuntu 執行測試與建置。整合版本 `ede56ef` 已通過各平台 **33 項單元測試 + 22 項整合測試**；後續狀態請以上方 CI 徽章為準。

Headless App／播放器不連接作業系統音訊裝置，測試涵蓋音訊資料、生成結果來源、劇本轉入時間軸與混音邏輯。API 錯誤處理測試可能需要網路，但這些結果不代表已完成真實配音或硬體播放驗收。

| 檔案 | 職責 |
| --- | --- |
| [src/app.rs](src/app.rs) | 主視窗、分頁、生成結果與跨工作區流程 |
| [src/api.rs](src/api.rs) | OpenRouter 請求與錯誤處理 |
| [src/audio.rs](src/audio.rs) | 播放、解碼與 WAV 匯出 |
| [src/multi_speech.rs](src/multi_speech.rs) | 多角色設定、逐句生成與拼接 |
| [src/storytelling.rs](src/storytelling.rs) | 體裁、導演建議、劇本解析與 QA |
| [src/timeline.rs](src/timeline.rs) | 軌道、片段、波形、剪輯與混音 |
| [src/file_manager.rs](src/file_manager.rs) | 作品庫管理 |
| [tests/integration_tests.rs](tests/integration_tests.rs) | 整合測試 |

更多設計變更與人工驗收項目：[使用體驗與整合說明](docs/UX-REDESIGN.md)。

## GitHub 自動建置與發布

[Build downloadable apps](https://github.com/stevenke1981/fish-s2pro-tts/actions/workflows/release.yml) 會先執行測試，再建置 Release 執行檔：

| 觸發方式 | 結果 |
| --- | --- |
| 推送至 `main` | 建置兩個平台，更新公開的 `preview` 預發行版 |
| 推送 `v*` 版本標籤 | 建置並建立對應 GitHub Release；含連字號的標籤視為預發行版 |
| Actions → Run workflow | 手動建置；選擇 `main` 時也會更新 Preview |

每次成功的平台建置都會上傳 Actions artifacts，保留 30 天。下載 Actions artifacts 需要登入 GitHub；公開 Releases 附件適合直接提供給一般使用者。

發布版本時，維護者可在確認提交後執行：

```bash
git tag v0.1.0
git push origin v0.1.0
```

版本號僅為示例，請使用尚未存在、且與發布內容相符的標籤。兩個平台的建置都成功後才會發布 Release；任何一個平台失敗，都不會更新公開的 Preview。
