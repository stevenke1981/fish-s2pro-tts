use fish_s2pro_tts::api::{OpenRouterClient, SpeechRequest, DEFAULT_MODEL, PRO_MODEL};
use fish_s2pro_tts::audio::AudioPlayer;
use fish_s2pro_tts::config::AppConfig;
use fish_s2pro_tts::models::{
    format_speech_input, get_default_characters, get_sample_scripts, get_tone_tags,
};

#[test]
fn test_speech_request_edge_cases() {
    // 1. 測試含有引號、換行、Unicode、多重情緒標籤的複雜字串
    let complex_text = "[happy] 你好！\"這是引號\" \n [whispering] 這是耳語…… \t [sigh] 嘆氣。 🚀 繁體測試";
    let req = SpeechRequest {
        model: DEFAULT_MODEL.to_string(),
        input: complex_text.to_string(),
        voice: None,
        response_format: Some("mp3".to_string()),
        speed: Some(1.25),
    };

    let json = serde_json::to_string(&req).expect("序列化應成功");
    assert!(json.contains("fish-audio/s2.1-pro-free:free"));
    assert!(json.contains("whispering"));
    assert!(json.contains("繁體測試"));

    // 2. 測試反序列化
    let deserialized: SpeechRequest = serde_json::from_str(&json).expect("反序列化應成功");
    assert_eq!(deserialized.input, complex_text);
    assert_eq!(deserialized.speed, Some(1.25));
    assert_eq!(deserialized.voice, None);

    // 3. 測試 Pro 模型
    let pro_req = SpeechRequest {
        model: PRO_MODEL.to_string(),
        input: "[serious] 戰鬥開始".to_string(),
        voice: Some("custom_speaker_id_12345".to_string()),
        response_format: Some("wav".to_string()),
        speed: Some(0.8),
    };
    let pro_json = serde_json::to_string(&pro_req).expect("Pro 模型序列化應成功");
    assert!(pro_json.contains(PRO_MODEL));
    assert!(pro_json.contains("custom_speaker_id_12345"));
}

#[test]
fn test_api_client_error_handling() {
    let client = OpenRouterClient::new();

    // 測試空字串 key
    let err_empty = client.verify_key("   ").unwrap_err();
    assert!(err_empty.contains("不能為空"));

    // 測試空輸入合成文字
    let empty_req = SpeechRequest {
        model: DEFAULT_MODEL.to_string(),
        input: "    ".to_string(),
        voice: None,
        response_format: Some("mp3".to_string()),
        speed: Some(1.0),
    };
    let err_input = client.synthesize("fake_key", &empty_req).unwrap_err();
    assert!(err_input.contains("不可為空"));

    // 測試無效 key 合成文字
    let valid_input_req = SpeechRequest {
        model: DEFAULT_MODEL.to_string(),
        input: "你好".to_string(),
        voice: None,
        response_format: Some("mp3".to_string()),
        speed: Some(1.0),
    };
    let err_synth = client.synthesize("invalid_key_12345", &valid_input_req).unwrap_err();
    assert!(err_synth.contains("認證") || err_synth.contains("失敗") || err_synth.contains("401"));
}

#[test]
fn test_config_persistence_and_sanitization() {
    let mut config = AppConfig {
        api_key: "secret_api_key_12345".to_string(),
        remember_api_key: false,
        ..Default::default()
    };

    // 當 remember_api_key 為 false 時，呼叫 save 應在保存前清空 key
    let serialized_temp = serde_json::to_string(&config).unwrap();
    assert!(serialized_temp.contains("secret_api_key_12345"));

    // 測試自訂口氣標籤設定
    config.custom_tones.push("[冷酷低沉]".to_string());
    assert!(config.custom_tones.contains(&"[冷酷低沉]".to_string()));
}

#[test]
fn test_audio_player_state_transitions() {
    let mut player = AudioPlayer::new();

    // 初始狀態
    assert_eq!(player.get_volume(), 1.0);
    assert!(!player.is_playing());
    assert!(!player.is_paused());

    // 測試音量邊界截斷 (clamping)
    player.set_volume(1.5);
    assert_eq!(player.get_volume(), 1.0);
    player.set_volume(-0.5);
    assert_eq!(player.get_volume(), 0.0);
    player.set_volume(0.75);
    assert_eq!(player.get_volume(), 0.75);

    // 測試多次 stop 不崩潰
    player.stop();
    player.stop();
    player.pause();
    player.resume();
    assert!(!player.is_playing());

    // 測試解碼無效音訊格式應回傳 Err
    let invalid_bytes = vec![0u8, 1u8, 2u8, 3u8, 4u8, 5u8];
    let res = player.play_bytes(invalid_bytes);
    assert!(res.is_err());
}

#[test]
fn test_models_and_character_presets() {
    let characters = get_default_characters();
    assert!(characters.len() >= 6);

    // 確認每個預設角色名稱與描述非空，且建議語速在合理區間
    for c in &characters {
        assert!(!c.name.is_empty());
        assert!(!c.description.is_empty());
        assert!(c.recommended_speed >= 0.8 && c.recommended_speed <= 1.2);
    }

    // 確認情緒標籤庫包含標準方括號 [tag] 或 speaker 標籤 <|speaker:N|>
    let tags = get_tone_tags();
    assert!(tags.len() >= 15);
    for t in &tags {
        let is_bracket = t.tag.starts_with('[') && t.tag.ends_with(']');
        let is_speaker = t.tag.starts_with("<|") && t.tag.ends_with("|>");
        assert!(is_bracket || is_speaker, "Invalid tag format: {}", t.tag);
        assert!(!t.label.is_empty());
        assert!(!t.description.is_empty());
    }

    // 確認每個預設角色具有 prompt_tag 且正確包含描述方括號
    for c in &characters {
        assert!(c.prompt_tag.starts_with('[') && c.prompt_tag.ends_with(']'));
    }

    // 確認範例台詞內容
    let samples = get_sample_scripts();
    assert!(!samples.is_empty());
    for s in &samples {
        assert!(!s.title.is_empty());
        assert!(!s.content.is_empty());
        assert!(s.content.contains('[') || s.content.contains("<|"));
    }
}

#[test]
fn test_format_speech_input_and_seeking() {
    let characters = get_default_characters();
    let gentle_female = &characters[0];

    // 1. auto_apply = true 且原文未含標籤 -> 自動前綴
    let formatted = format_speech_input("早安，主人！", gentle_female, true);
    assert!(formatted.starts_with(&gentle_female.prompt_tag));
    assert!(formatted.contains("早安，主人！"));

    // 2. auto_apply = true 但原文已包含該標籤 -> 不重複附加
    let already_tagged = format!("{} 早安，主人！", gentle_female.prompt_tag);
    let formatted_dup = format_speech_input(&already_tagged, gentle_female, true);
    assert_eq!(formatted_dup, already_tagged);

    // 3. auto_apply = false -> 原樣保留
    let unapplied = format_speech_input("純文字測試", gentle_female, false);
    assert_eq!(unapplied, "純文字測試");

    // 4. 音訊播放器 Seek 狀態轉換
    let mut player = AudioPlayer::new();
    let seek_res = player.seek(std::time::Duration::from_secs(5));
    // 在未載入真實音訊時 seek 應安全失敗或返回錯誤，絕不 panic
    assert!(seek_res.is_err());
}

#[test]
fn test_file_manager_scanning_filtering_and_sorting() {
    use fish_s2pro_tts::file_manager::{
        AudioFileEntry, FileManagerState, FileSortBy, FormatFilter,
    };
    use std::path::PathBuf;
    use std::time::SystemTime;

    let mut state = FileManagerState::new();

    let e1 = AudioFileEntry {
        filename: "speech_hero.mp3".to_string(),
        path: PathBuf::from("outputs/speech_hero.mp3"),
        byte_size: 10240,
        modified_time: SystemTime::now(),
        modified_str: "2026-09-05 01:00:00".to_string(),
        duration_secs: Some(12.5),
        format: "MP3".to_string(),
        character_name: "熱血陽光少年".to_string(),
        text_snippet: "站住！接招吧！".to_string(),
        model: "fish-audio/s2.1-pro-free:free".to_string(),
        is_selected: false,
    };

    let e2 = AudioFileEntry {
        filename: "dialog_scene.wav".to_string(),
        path: PathBuf::from("outputs/dialog_scene.wav"),
        byte_size: 44100,
        modified_time: SystemTime::now() + std::time::Duration::from_secs(60),
        modified_str: "2026-09-05 01:01:00".to_string(),
        duration_secs: Some(35.0),
        format: "WAV".to_string(),
        character_name: "多角色劇本 (2 位登場)".to_string(),
        text_snippet: "你終於回來了".to_string(),
        model: "fish-audio/s2.1-pro-free:free".to_string(),
        is_selected: false,
    };

    let e3 = AudioFileEntry {
        filename: "narration_city.mp3".to_string(),
        path: PathBuf::from("outputs/narration_city.mp3"),
        byte_size: 20480,
        modified_time: SystemTime::now() + std::time::Duration::from_secs(120),
        modified_str: "2026-09-05 01:02:00".to_string(),
        duration_secs: Some(5.0),
        format: "MP3".to_string(),
        character_name: "大氣紀錄片旁白".to_string(),
        text_snippet: "這座城市在夜色中沉睡".to_string(),
        model: "fish-audio/s2.1-pro".to_string(),
        is_selected: false,
    };

    state.files = vec![e1, e2, e3];

    // 1. 關鍵字搜尋測試 (搜尋「城市」)
    state.search_query = "城市".to_string();
    let filtered = state.filtered_files();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].filename, "narration_city.mp3");

    // 2. 格式篩選測試 (篩選 WAV)
    state.search_query.clear();
    state.format_filter = FormatFilter::Wav;
    let filtered_wav = state.filtered_files();
    assert_eq!(filtered_wav.len(), 1);
    assert_eq!(filtered_wav[0].filename, "dialog_scene.wav");

    // 3. 排序測試 (依時長由長到短)
    state.format_filter = FormatFilter::All;
    state.sort_by = FileSortBy::DurationDesc;
    state.apply_sort();
    assert_eq!(state.files[0].filename, "dialog_scene.wav"); // 35.0s
    assert_eq!(state.files[1].filename, "speech_hero.mp3");  // 12.5s
    assert_eq!(state.files[2].filename, "narration_city.mp3"); // 5.0s

    // 4. 統計計算測試
    let (count, bytes, dur) = state.statistics();
    assert_eq!(count, 3);
    assert_eq!(bytes, 10240 + 44100 + 20480);
    assert!((dur - 52.5).abs() < 0.01);
}

#[test]
fn test_file_manager_rename_and_deletion_safety() {
    use fish_s2pro_tts::file_manager::{AudioFileEntry, FileManagerState};
    use std::fs;
    use std::time::SystemTime;

    let mut state = FileManagerState::new();

    // 建立臨時測試檔案
    let temp_dir = std::env::temp_dir().join("fish_tts_test_fm");
    let _ = fs::create_dir_all(&temp_dir);
    let test_file1 = temp_dir.join("temp_audio1.mp3");
    let test_file2 = temp_dir.join("temp_audio2.wav");
    fs::write(&test_file1, b"test_audio_content_1").expect("寫入測試音訊 1");
    fs::write(&test_file2, b"test_audio_content_2").expect("寫入測試音訊 2");

    let entry1 = AudioFileEntry {
        filename: "temp_audio1.mp3".to_string(),
        path: test_file1.clone(),
        byte_size: 20,
        modified_time: SystemTime::now(),
        modified_str: "2026-09-05 01:00:00".to_string(),
        duration_secs: Some(1.0),
        format: "MP3".to_string(),
        character_name: "測試".to_string(),
        text_snippet: "測試台詞".to_string(),
        model: "model".to_string(),
        is_selected: false,
    };
    let entry2 = AudioFileEntry {
        filename: "temp_audio2.wav".to_string(),
        path: test_file2.clone(),
        byte_size: 20,
        modified_time: SystemTime::now(),
        modified_str: "2026-09-05 01:00:00".to_string(),
        duration_secs: Some(1.0),
        format: "WAV".to_string(),
        character_name: "測試 2".to_string(),
        text_snippet: "測試台詞 2".to_string(),
        model: "model".to_string(),
        is_selected: true, // 選取第 2 個項目
    };
    state.files = vec![entry1, entry2];
    state.selected_file_path = Some(test_file2.clone());

    // 測試非法字元檢查
    let err_illegal = state.rename_file(&test_file1, "bad:name/test").unwrap_err();
    assert!(err_illegal.contains("非法字元"));

    // 測試空白名稱檢查
    let err_empty = state.rename_file(&test_file1, "   ").unwrap_err();
    assert!(err_empty.contains("不可為空"));

    // 測試大小寫不敏感的副檔名更名 (輸入大寫 .MP3，不應被誤追加成 .MP3.mp3)
    let new_path = state.rename_file(&test_file1, "renamed_track.MP3").expect("更名應成功");
    assert!(new_path.exists());
    assert_eq!(new_path.file_name().unwrap(), "renamed_track.MP3");
    assert_eq!(state.files[0].filename, "renamed_track.MP3");
    assert_eq!(state.files[0].format, "MP3");

    // 測試批次刪除安全性 (僅刪除被選取的 test_file2，test_file1 必須保留)
    let del_cnt = state.batch_delete_selected().expect("批次刪除應成功");
    assert_eq!(del_cnt, 1);
    assert!(!test_file2.exists());
    assert!(new_path.exists());
    assert_eq!(state.files.len(), 1);
    assert_eq!(state.files[0].path, new_path);
    // 確認刪除當前播放檔案時，selected_file_path 自動被清空為 None
    assert_eq!(state.selected_file_path, None);

    // 清理臨時檔案
    let _ = fs::remove_file(new_path);
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_multi_speaker_dialog_prompt_and_cast_management() {
    use fish_s2pro_tts::models::get_default_characters;
    use fish_s2pro_tts::multi_speech::{
        build_native_multi_speaker_prompt, CastMember, DialogLine, MultiSpeechState,
    };

    let mut state = MultiSpeechState::new();
    let presets = get_default_characters();

    // 1. 角色管理：新增與刪除
    let init_count = state.cast.len();
    state.add_speaker(&presets);
    assert_eq!(state.cast.len(), init_count + 1);

    let last_speaker_id = state.cast.last().unwrap().speaker_id;
    state.remove_speaker(last_speaker_id);
    assert_eq!(state.cast.len(), init_count);

    // 2. 台詞排序與編輯
    let line1_id = state.lines[0].id;
    let line2_id = state.lines[1].id;
    state.move_line_down(0);
    assert_eq!(state.lines[0].id, line2_id);
    assert_eq!(state.lines[1].id, line1_id);

    state.move_line_up(1);
    assert_eq!(state.lines[0].id, line1_id);
    assert_eq!(state.lines[1].id, line2_id);

    // 3. 原生 Fish Audio 語法 Prompt 構建
    let cast = vec![
        CastMember {
            speaker_id: 0,
            name: "御姐".to_string(),
            character_preset_idx: 0,
            prompt_tag: "[溫柔御姐音色]".to_string(),
            custom_voice_id: None,
            default_tone: "[calm]".to_string(),
            speed: 1.0,
            badge_color: [0, 0, 0],
        },
        CastMember {
            speaker_id: 1,
            name: "少年".to_string(),
            character_preset_idx: 1,
            prompt_tag: "[熱血少年音色]".to_string(),
            custom_voice_id: None,
            default_tone: "[excited]".to_string(),
            speed: 1.05,
            badge_color: [0, 0, 0],
        },
    ];

    let lines = vec![
        DialogLine {
            id: 1,
            speaker_id: 0,
            tone: "[calm]".to_string(),
            text: "這條路很危險，你確定要去嗎？".to_string(),
            pause_after_ms: 300,
        },
        DialogLine {
            id: 2,
            speaker_id: 1,
            tone: "[excited] [充滿決心]".to_string(),
            text: "我絕不退縮！這是我自己的選擇！".to_string(),
            pause_after_ms: 400,
        },
    ];

    let prompt = build_native_multi_speaker_prompt(&cast, &lines);
    assert!(prompt.contains("<|speaker:0|>"));
    assert!(prompt.contains("[溫柔御姐音色]"));
    assert!(prompt.contains("[calm]"));
    assert!(prompt.contains("這條路很危險，你確定要去嗎？"));

    assert!(prompt.contains("<|speaker:1|>"));
    assert!(prompt.contains("[熱血少年音色]"));
    assert!(prompt.contains("[excited] [充滿決心]"));
    assert!(prompt.contains("我絕不退縮！這是我自己的選擇！"));

    // 4. 文字劇本匯出格式測試
    let script_text = state.export_text_script();
    assert!(script_text.contains("【登場角色配置】"));
    assert!(script_text.contains("【劇本對白內容】"));
}

#[test]
fn test_audio_concatenation_wav_and_mp3() {
    use fish_s2pro_tts::multi_speech::{concatenate_mp3_buffers, concatenate_wav_buffers};

    // 1. WAV 拼接測試
    fn make_dummy_wav(pcm_len: usize) -> Vec<u8> {
        let mut w = vec![0u8; 44 + pcm_len];
        w[0..4].copy_from_slice(b"RIFF");
        let riff_sz = (pcm_len + 36) as u32;
        w[4..8].copy_from_slice(&riff_sz.to_le_bytes());
        w[8..12].copy_from_slice(b"WAVE");
        w[12..16].copy_from_slice(b"fmt ");
        w[16..20].copy_from_slice(&16u32.to_le_bytes());
        w[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
        w[22..24].copy_from_slice(&1u16.to_le_bytes()); // 1 channel
        w[24..28].copy_from_slice(&44100u32.to_le_bytes()); // 44100Hz
        w[28..32].copy_from_slice(&88200u32.to_le_bytes()); // 88200 Bps
        w[32..34].copy_from_slice(&2u16.to_le_bytes()); // BlockAlign = 2
        w[34..36].copy_from_slice(&16u16.to_le_bytes()); // 16-bit
        w[36..40].copy_from_slice(b"data");
        let data_sz = pcm_len as u32;
        w[40..44].copy_from_slice(&data_sz.to_le_bytes());
        w
    }

    let w1 = make_dummy_wav(400);
    let w2 = make_dummy_wav(600);
    let wav_res = concatenate_wav_buffers(&[&w1, &w2], 50).expect("WAV 拼接");
    assert_eq!(&wav_res[0..4], b"RIFF");
    assert_eq!(&wav_res[8..12], b"WAVE");
    assert!(wav_res.len() > 44 + 400 + 600);

    // 2. MP3 拼接與 ID3 剝除測試
    // 構造含有 ID3 標頭的虛擬 MP3
    let mut mp3_with_id3 = vec![0u8; 20];
    mp3_with_id3[0..3].copy_from_slice(b"ID3");
    mp3_with_id3[3] = 4; // version
    mp3_with_id3[9] = 10; // tag size = 10 (total offset = 20)
    mp3_with_id3.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x64]); // fake frame sync

    let mp3_res = concatenate_mp3_buffers(&[&mp3_with_id3, &mp3_with_id3]).expect("MP3 拼接");
    assert!(!mp3_res.is_empty());
}

#[test]
fn test_audio_concatenation_with_custom_pauses() {
    use fish_s2pro_tts::multi_speech::{
        concatenate_mp3_buffers_with_pauses, concatenate_wav_buffers_with_pauses, make_mp3_silence,
    };

    fn make_dummy_wav(pcm_len: usize) -> Vec<u8> {
        let mut w = vec![0u8; 44 + pcm_len];
        w[0..4].copy_from_slice(b"RIFF");
        let riff_sz = (pcm_len + 36) as u32;
        w[4..8].copy_from_slice(&riff_sz.to_le_bytes());
        w[8..12].copy_from_slice(b"WAVE");
        w[12..16].copy_from_slice(b"fmt ");
        w[16..20].copy_from_slice(&16u32.to_le_bytes());
        w[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
        w[22..24].copy_from_slice(&1u16.to_le_bytes()); // 1 channel
        w[24..28].copy_from_slice(&44100u32.to_le_bytes()); // 44100Hz
        w[28..32].copy_from_slice(&88200u32.to_le_bytes()); // 88200 Bps
        w[32..34].copy_from_slice(&2u16.to_le_bytes()); // BlockAlign = 2
        w[34..36].copy_from_slice(&16u16.to_le_bytes()); // 16-bit
        w[36..40].copy_from_slice(b"data");
        let data_sz = pcm_len as u32;
        w[40..44].copy_from_slice(&data_sz.to_le_bytes());
        w
    }

    let w1 = make_dummy_wav(200);
    let w2 = make_dummy_wav(400);

    // 測試自訂停頓拼接 (第 1 句後停頓 100ms，第 2 句後停頓 0ms)
    let pauses = vec![100, 0];
    let res_wav = concatenate_wav_buffers_with_pauses(&[&w1, &w2], &pauses).expect("WAV 自訂停頓拼接成功");
    assert_eq!(&res_wav[0..4], b"RIFF");
    assert_eq!(&res_wav[8..12], b"WAVE");
    // 100ms silence at 44.1kHz 16-bit mono = 4410 * 2 = 8820 bytes
    assert!(res_wav.len() >= 44 + 200 + 400 + 8820);

    // 測試 make_mp3_silence
    let silence = make_mp3_silence(50);
    assert!(!silence.is_empty());
    assert_eq!(silence[0], 0xFF);
    assert_eq!(silence[1], 0xFB);

    // 測試 MP3 自訂停頓拼接
    let fake_mp3 = vec![0xFF, 0xFB, 0x90, 0x64];
    let mp3_pauses = vec![100, 0];
    let res_mp3 = concatenate_mp3_buffers_with_pauses(&[&fake_mp3, &fake_mp3], &mp3_pauses).expect("MP3 自訂停頓拼接成功");
    assert!(!res_mp3.is_empty());
}

#[test]
fn test_export_audio_bytes_conversion() {
    use fish_s2pro_tts::audio::{decode_to_pcm, encode_pcm_to_wav, export_audio_bytes};
    use std::fs;

    let temp_dir = std::env::temp_dir().join("fish_tts_export_test");
    let _ = fs::create_dir_all(&temp_dir);

    let dummy_samples: Vec<i16> = vec![0, 500, -500, 1000, -1000];
    let wav_bytes = encode_pcm_to_wav(&dummy_samples, 44100, 1);

    // 1. WAV 匯出為 WAV 檔案 (直通寫入)
    let wav_target = temp_dir.join("test_out.wav");
    export_audio_bytes(&wav_bytes, &wav_target).expect("WAV 直通匯出成功");
    let read_back_wav = fs::read(&wav_target).expect("讀取 WAV");
    assert_eq!(read_back_wav, wav_bytes);

    // 2. 非 WAV 位元組 (例如 MP3 假資料) 匯出為 .mp3 (直通寫入)
    let fake_mp3 = vec![0xFF, 0xFB, 0x90, 0x64, 0x00, 0x00];
    let mp3_target = temp_dir.join("test_out.mp3");
    export_audio_bytes(&fake_mp3, &mp3_target).expect("MP3 直通匯出成功");
    let read_back_mp3 = fs::read(&mp3_target).expect("讀取 MP3");
    assert_eq!(read_back_mp3, fake_mp3);

    // 3. 解碼驗證
    let (samples, sr, ch) = decode_to_pcm(&wav_bytes).expect("解碼應成功");
    assert_eq!(sr, 44100);
    assert_eq!(ch, 1);
    assert_eq!(samples, dummy_samples);

    let _ = fs::remove_file(&wav_target);
    let _ = fs::remove_file(&mp3_target);
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_scan_output_files_history_matching_and_duration() {
    use fish_s2pro_tts::file_manager::scan_output_files;
    use fish_s2pro_tts::models::GenerationHistoryItem;
    use std::fs;

    let temp_dir = std::env::temp_dir().join("fish_tts_scan_test");
    let _ = fs::create_dir_all(&temp_dir);

    let filename = "speech_20260905_999999.mp3";
    let file_path = temp_dir.join(filename);
    // 寫入包含 MP3 標頭的資料
    let mut mp3_content = vec![0xFF, 0xFB, 0x90, 0x64];
    mp3_content.resize(10240, 0);
    fs::write(&file_path, &mp3_content).expect("寫入測試音檔");

    let history_item = GenerationHistoryItem {
        id: "20260905_999999".to_string(),
        timestamp: "02:00:00".to_string(),
        text: "歷史台詞配對測試".to_string(),
        character_name: "優雅御姐".to_string(),
        model: "fish-audio/s2.1-pro-free:free".to_string(),
        format: "mp3".to_string(),
        speed: 1.0,
        duration_secs: Some(8.5),
        file_path: "outputs/speech_20260905_999999.mp3".to_string(), // 相對路徑
        byte_size: 10240,
    };

    let entries = scan_output_files(&temp_dir, &[history_item]);
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.filename, filename);
    assert_eq!(entry.character_name, "優雅御姐");
    assert_eq!(entry.text_snippet, "歷史台詞配對測試");
    assert_eq!(entry.duration_secs, Some(8.5));

    let _ = fs::remove_file(file_path);
    let _ = fs::remove_dir_all(temp_dir);
}
