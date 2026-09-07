# Agent CLI 與 MCP

發行壓縮包包含桌面程式與 `fish-s2pro-tts-cli`（Windows 為 `.exe`）。CLI / MCP 不需要顯示器或音訊輸出裝置。

## 編譯與離線預覽

```powershell
cargo build --release --locked --no-default-features --bin fish-s2pro-tts-cli
.\target\release\fish-s2pro-tts-cli.exe status
.\target\release\fish-s2pro-tts-cli.exe catalog
.\target\release\fish-s2pro-tts-cli.exe prepare --text "[calm] 你好。" --output hello.mp3
```

Linux 使用 `./target/release/fish-s2pro-tts-cli`。`status`、`catalog`、`prepare` 與 `--dry-run` 不呼叫服務商。服務啟動會建立輸出資料夾。

## 生成

由啟動環境提供 `OPENROUTER_API_KEY`，不要將金鑰放在命令參數、提交或聊天中。也可明確指定 `--config FILE` 讀取本機 GUI 設定；預設不搜尋設定檔。

確認模型、文字、輸出格式、費用與額度後，才執行：

```powershell
.\fish-s2pro-tts-cli.exe --output-dir .\audio synthesize --text-file .\script.txt --output scene-01.mp3
.\fish-s2pro-tts-cli.exe --output-dir .\audio batch --input .\batch.json --dry-run
```

批次 JSON 格式：

```json
{"items":[{"text":"[calm] 第一段。","output":"01.mp3"},{"text":"[excited] 第二段！","output":"02.mp3"}]}
```

確認後移除批次的 `--dry-run` 才會生成。批次依序執行，第一個錯誤即停止，回傳已完成輸出，不自動重試。輸出限指定資料夾內的單一檔名、不覆寫；支援 mp3、wav、pcm，副檔名必須一致。每段含標籤最多 4,000 字元，批次最多 100 段、總計 100,000 字元。

## MCP stdio 設定

在支援 `mcpServers` 格式的客戶端使用以下範例；替換為實際絕對路徑，並讓客戶端程序繼承金鑰環境變數：

```json
{
  "mcpServers": {
    "fish-tts": {
      "command": "E:/fish-s2pro-tts/target/release/fish-s2pro-tts-cli.exe",
      "args": ["--output-dir", "E:/fish-s2pro-tts/fish_tts_output", "mcp", "--stdio"]
    }
  }
}
```

各客戶端的設定格式可能不同。此伺服器採 [MCP stdio 傳輸](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)，支援協商 `2025-11-25`、`2025-06-18`、`2025-03-26`。

工具包含 `fish_status`、`fish_catalog`、`fish_prepare`、`fish_synthesize`、`fish_batch_synthesize`、`fish_job_status`、`fish_job_cancel`。先預覽與確認，再生成。生成立即回傳 job ID，使用 status 輪詢；同時最多一個生成工作。

工作只存在本次 MCP 程序中，不支援跨重啟續跑。關閉 stdin 會取消後續工作並等待目前 HTTP 請求結束（最長約 90 秒）；已送出的請求仍可能計費。stdout 僅輸出 JSON-RPC，診斷使用 stderr。

## 驗證範圍

離線測試涵蓋 CLI 子程序、MCP 握手與工具呼叫、路徑限制、不覆寫、錯誤遮罩、批次與取消。生成測試使用注入的假後端，不代表真實付費模型已驗證。音訊回傳僅做格式標頭與大小檢查，未完整解碼；實際生成後仍需檢查檔案可播放性及內容。
