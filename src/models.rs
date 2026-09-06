use serde::{Deserialize, Serialize};

/// 角色預設設定
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterPreset {
    pub name: String,
    pub description: String,
    /// 角色聲線提示詞（注入 Fish Audio S2.1 的自然語言音色標籤）
    #[serde(default)]
    pub prompt_tag: String,
    /// 自訂 Fish Audio reference ID (若有已於 fish.audio 建立之聲音模型 ID)
    pub voice_id: Option<String>,
    pub recommended_speed: f32,
    pub default_tone: Option<String>,
}

/// 口氣與對白標籤分類
#[derive(Clone, Debug, PartialEq)]
pub enum ToneCategory {
    Emotion, // 基礎情緒
    Action,  // 副語言與語音動作
    Style,   // 說話風格
    Speaker, // 多角色/說話者切換
    Custom,  // 自訂標籤
}

/// 口氣/情緒/角色標籤
#[derive(Clone, Debug, PartialEq)]
pub struct ToneTag {
    pub tag: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub category: ToneCategory,
}

/// 範例腳本
#[derive(Clone, Debug, PartialEq)]
pub struct SampleScript {
    pub title: &'static str,
    pub category: &'static str,
    pub content: &'static str,
    pub suggested_character: &'static str,
}

/// 歷史紀錄項目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationHistoryItem {
    pub id: String,
    pub timestamp: String,
    pub text: String,
    pub character_name: String,
    pub model: String,
    pub format: String,
    pub speed: f32,
    pub duration_secs: Option<f32>,
    pub file_path: String,
    pub byte_size: usize,
}

/// 取得內建角色列表
pub fn get_default_characters() -> Vec<CharacterPreset> {
    vec![
        CharacterPreset {
            name: "Fish 原生標準聲線".to_string(),
            description: "Fish Audio S2.1 官方基礎標準音色，自然流暢，適用廣泛題材".to_string(),
            prompt_tag: "[calm]".to_string(),
            voice_id: None,
            recommended_speed: 1.0,
            default_tone: Some("[calm]".to_string()),
        },
        CharacterPreset {
            name: "溫柔知性御姐".to_string(),
            description: "溫暖柔和、優雅親切的成熟女性聲音，適合睡前故事與情感陪伴".to_string(),
            prompt_tag: "[溫柔知性御姐音色]".to_string(),
            voice_id: None,
            recommended_speed: 0.95,
            default_tone: Some("[calm] [溫柔細語]".to_string()),
        },
        CharacterPreset {
            name: "活力元氣少女".to_string(),
            description: "明朗熱情、充滿朝氣的動漫少女音，適合遊戲配音與歡樂互動".to_string(),
            prompt_tag: "[活力元氣少女音色]".to_string(),
            voice_id: None,
            recommended_speed: 1.05,
            default_tone: Some("[excited] [happy]".to_string()),
        },
        CharacterPreset {
            name: "沉穩磁性青年".to_string(),
            description: "低沉穩健、深具說服力的青年男性聲音，具備信賴感與精英氣質".to_string(),
            prompt_tag: "[沉穩磁性男性音色]".to_string(),
            voice_id: None,
            recommended_speed: 1.0,
            default_tone: Some("[serious]".to_string()),
        },
        CharacterPreset {
            name: "熱血陽光少年".to_string(),
            description: "清亮富有爆發力的熱血少年音，適合冒險、戰鬥與激勵台詞".to_string(),
            prompt_tag: "[熱血清亮少年音色]".to_string(),
            voice_id: None,
            recommended_speed: 1.05,
            default_tone: Some("[excited]".to_string()),
        },
        CharacterPreset {
            name: "大氣紀錄片旁白".to_string(),
            description: "莊重深邃、字正腔圓的大師級播音主持音色，極具畫面感".to_string(),
            prompt_tag: "[大氣專業紀錄片旁白播音員]".to_string(),
            voice_id: None,
            recommended_speed: 0.95,
            default_tone: Some("[calm] [廣播播音腔]".to_string()),
        },
        CharacterPreset {
            name: "智慧科技助手".to_string(),
            description: "清晰、高效、親和力高的現代 AI 語音助理風格".to_string(),
            prompt_tag: "[清晰親和科技AI助手]".to_string(),
            voice_id: None,
            recommended_speed: 1.0,
            default_tone: Some("[calm]".to_string()),
        },
        CharacterPreset {
            name: "傲嬌大小姐".to_string(),
            description: "微帶驕傲矜持、起伏生動鮮明的經典二次元傲嬌少女聲線".to_string(),
            prompt_tag: "[傲嬌大小姐少女音色]".to_string(),
            voice_id: None,
            recommended_speed: 1.0,
            default_tone: Some("[sarcastic] [proud]".to_string()),
        },
        CharacterPreset {
            name: "慈祥和藹老者".to_string(),
            description: "緩慢、慈祥且富含人生歷練的老人聲音，適合長者敘事與童話說書".to_string(),
            prompt_tag: "[慈祥和藹的老人音色]".to_string(),
            voice_id: None,
            recommended_speed: 0.9,
            default_tone: Some("[說故事口吻]".to_string()),
        },
        CharacterPreset {
            name: "冷酷神秘刺客".to_string(),
            description: "冷靜漠然、壓抑氣場的暗影刺客聲線，適合反派或孤狼角色".to_string(),
            prompt_tag: "[冷酷低沉神秘刺客]".to_string(),
            voice_id: None,
            recommended_speed: 0.95,
            default_tone: Some("[冷靜理性] [whispering]".to_string()),
        },
    ]
}

/// 取得支援的情緒、口氣與對白標籤庫
pub fn get_tone_tags() -> Vec<ToneTag> {
    vec![
        // 基礎情緒
        ToneTag {
            tag: "[happy]",
            label: "歡喜 / 開心",
            description: "開朗、明亮的情感語氣",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[sad]",
            label: "悲傷 / 難過",
            description: "低沉、壓抑的悲傷語氣",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[angry]",
            label: "憤怒 / 生氣",
            description: "激烈、嚴厲、具爆發力的怒火",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[excited]",
            label: "興奮 / 激動",
            description: "雀躍高亢、充滿激情的語調",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[calm]",
            label: "平靜 / 溫和",
            description: "平穩祥和、放鬆舒適的講話節奏",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[whispering]",
            label: "耳語 / 低語",
            description: "微弱輕柔的氣音，如在耳邊說秘密",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[serious]",
            label: "嚴肅 / 認真",
            description: "鄭重其事、堅定不移的態度",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[sarcastic]",
            label: "諷刺 / 嘲諷",
            description: "微帶輕蔑、略帶譏諷的調侃語氣",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[crying]",
            label: "哭腔 / 抽泣",
            description: "帶著顫抖抽泣的極度傷心語氣",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[fearful]",
            label: "恐懼 / 害怕",
            description: "心驚膽戰、慌張失措的語氣",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[shy]",
            label: "害羞 / 靦腆",
            description: "略帶猶豫嬌羞的語調",
            category: ToneCategory::Emotion,
        },
        ToneTag {
            tag: "[proud]",
            label: "自豪 / 得意",
            description: "自信滿滿、沾沾自喜的聲調",
            category: ToneCategory::Emotion,
        },
        // 副語言動作
        ToneTag {
            tag: "[sigh]",
            label: "嘆氣 / 嘆息",
            description: "發出無奈或釋懷的嘆氣聲",
            category: ToneCategory::Action,
        },
        ToneTag {
            tag: "[chuckle]",
            label: "輕笑 / 偷笑",
            description: "發出細微好笑的忍俊不禁聲",
            category: ToneCategory::Action,
        },
        ToneTag {
            tag: "[gasp]",
            label: "倒抽一口氣",
            description: "驚訝或被嚇到時的吸氣聲",
            category: ToneCategory::Action,
        },
        ToneTag {
            tag: "[pant]",
            label: "喘氣 / 喘息",
            description: "劇烈運動或緊張時的急促呼吸",
            category: ToneCategory::Action,
        },
        ToneTag {
            tag: "[throat-clearing]",
            label: "清喉嚨",
            description: "開口前的清嗓子聲音",
            category: ToneCategory::Action,
        },
        ToneTag {
            tag: "[laughing]",
            label: "開懷大笑",
            description: "爽朗開懷的大笑聲",
            category: ToneCategory::Action,
        },
        // 自由風格語氣
        ToneTag {
            tag: "[溫柔細語]",
            label: "溫柔細語",
            description: "自然語言描述：極其溫和親切的語調",
            category: ToneCategory::Style,
        },
        ToneTag {
            tag: "[廣播播音腔]",
            label: "廣播播音腔",
            description: "自然語言描述：標準字正腔圓的專業主持風格",
            category: ToneCategory::Style,
        },
        ToneTag {
            tag: "[說故事口吻]",
            label: "說故事口吻",
            description: "自然語言描述：引人入勝、娓娓道來的繪本口吻",
            category: ToneCategory::Style,
        },
        ToneTag {
            tag: "[冷靜理性]",
            label: "冷靜理性",
            description: "自然語言描述：客觀、不帶感情色彩的分析口氣",
            category: ToneCategory::Style,
        },
        ToneTag {
            tag: "[充滿決心]",
            label: "充滿決心",
            description: "自然語言描述：堅定果敢、不容置疑的力量感",
            category: ToneCategory::Style,
        },
        // 多角色對白切換標籤 (Fish Audio S2.1 原生支援)
        ToneTag {
            tag: "<|speaker:0|>",
            label: "說話者 0 (角色A)",
            description: "Fish Audio S2.1 多角色對話標記：切換至說話者 0",
            category: ToneCategory::Speaker,
        },
        ToneTag {
            tag: "<|speaker:1|>",
            label: "說話者 1 (角色B)",
            description: "Fish Audio S2.1 多角色對話標記：切換至說話者 1",
            category: ToneCategory::Speaker,
        },
        ToneTag {
            tag: "<|speaker:2|>",
            label: "說話者 2 (角色C)",
            description: "Fish Audio S2.1 多角色對話標記：切換至說話者 2",
            category: ToneCategory::Speaker,
        },
    ]
}

/// 內建模擬示範腳本
pub fn get_sample_scripts() -> Vec<SampleScript> {
    vec![
        SampleScript {
            title: "城市夜幕（大氣旁白）",
            category: "旁白紀錄",
            content: "[calm] 夜幕低垂，城市的霓虹燈如繁星般漸次亮起。[sigh] 這座不眠之城，又迎來了一個安靜而漫長的夜晚。無數的故事在此交織，又在破曉時歸於平靜。",
            suggested_character: "大氣紀錄片旁白",
        },
        SampleScript {
            title: "最後的誓言（動漫冒險）",
            category: "動漫遊戲",
            content: "[angry] 站住！你的陰謀已經被徹底揭穿了！[excited] 大家一路走來的羈絆，絕不會被你這種人輕易斬斷！[serious] 今天，就是勝負的時刻！",
            suggested_character: "熱血陽光少年",
        },
        SampleScript {
            title: "深夜心聲（情感耳語）",
            category: "情感對白",
            content: "[whispering] 其實……我一直有些話想對你說。[shy] 只是每次看到你的眼睛，那些排練過無數次的話就全都忘光了……[chuckle] 你願意聽我慢慢說嗎？",
            suggested_character: "溫柔知性御姐",
        },
        SampleScript {
            title: "勝利大狂歡（元氣歡呼）",
            category: "日常互動",
            content: "[excited] 哇啊啊！我們成功了！真的拿到冠軍了！[happy] [chuckle] 看到沒有，我就說我們的特訓一定會奏效的！今晚一定要去吃大餐好好慶祝一番！",
            suggested_character: "活力元氣少女",
        },
        SampleScript {
            title: "雙人對白（男女合演演繹）",
            category: "多角色對話",
            content: "<|speaker:0|> [溫柔知性御姐音色] [calm] 你終於回來了，今天在外面辛苦了吧？晚餐已經熱好了。<|speaker:1|> [沉穩磁性男性音色] [happy] [chuckle] 只要回到家看見你，今天所有的疲憊都煙消雲散了。",
            suggested_character: "溫柔知性御姐",
        },
        SampleScript {
            title: "智能助理報時（科技功能）",
            category: "科技助手",
            content: "[calm] 早上好，主人。現在是上午八點整。今日天氣晴朗，氣溫攝氏二十四度，非常適合外出。您的今日日程已為您準備就緒，請隨時查閱。",
            suggested_character: "智慧科技助手",
        },
        SampleScript {
            title: "傲嬌日常（反差萌）",
            category: "動漫遊戲",
            content: "[sarcastic] 哼！你以為我是特地為你做這份便當的嗎？[proud] 只是早上材料買太多，不小心做多罷了！[shy] 難吃的話也不准吐出來，給我一粒不剩地吃完！",
            suggested_character: "傲嬌大小姐",
        },
        SampleScript {
            title: "童話故事（慈愛說書）",
            category: "長者故事",
            content: "[calm] [說故事口吻] 很久很久以前，在森林深處有一座被水晶守護的村莊。[chuckle] 孩子們每天在溪邊歡唱，直到一隻會說話的小狐狸悄悄走進了村口……",
            suggested_character: "慈祥和藹老者",
        },
        SampleScript {
            title: "武俠懸疑《風起塞外》",
            category: "故事體裁",
            content: "[low voice] [speaking slowly] 夜色沉沉，古道荒草叢生，四下沒有半點人聲。[low voice, dangerously calm] 交出玄鐵令，我留你全屍。[sigh] [calm] 退隱十年，終究還是躲不過這場血雨腥風。[whispering] 寒芒乍現，殘葉落處，風止。",
            suggested_character: "沉穩磁性青年",
        },
        SampleScript {
            title: "恐怖驚悚《午夜鐘聲》",
            category: "故事體裁",
            content: "[low voice] [speaking slowly] [mysterious] 整棟洋房空無一人，但空氣中卻飄著淡淡的潮濕霉味。[whispering] [nervous] 喂……有人在那裡嗎？不要開這種玩笑……[gasp] 咚……午夜十二點的鐘聲，毫無預警地敲響了第一聲。[whispering] [soft voice] 你終於……來陪我了……",
            suggested_character: "冷酷反派刺客",
        },
        SampleScript {
            title: "睡前童話《星光森林的小狐狸》",
            category: "故事體裁",
            content: "[soft voice] [warm] [speaking slowly] 月亮升起來了，柔和的銀光灑在整片安靜的森林上。[soft voice] [sigh] 今天走了一整天，星星看起來好溫暖呀。[warm storyteller tone] 小狐狸蜷縮在厚厚的苔蘚上，慢慢闔上了眼睛。晚安，做個好夢。",
            suggested_character: "溫柔知性御姐",
        },
        SampleScript {
            title: "奇幻史詩《巨龍王座的誓言》",
            category: "故事體裁",
            content: "[warm] [cinematic] [measured pacing] 一千年前沉睡的黑翼，在今日的狂風中再度遮蔽了蒼穹。[low voice] [loud voice] 凡人，汝之血肉，何以抵禦烈焰？[excited] [emphasis] 我們身後就是王國最後的希望，拔劍！為了誓言！",
            suggested_character: "大氣紀錄片旁白",
        },
    ]
}

/// 依角色與選項格式化最終發送給 Fish Audio S2.1 的合成文字
pub fn format_speech_input(raw_input: &str, character: &CharacterPreset, auto_apply_tag: bool) -> String {
    let trimmed = raw_input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if !auto_apply_tag || character.prompt_tag.trim().is_empty() {
        return trimmed.to_string();
    }

    let tag = character.prompt_tag.trim();
    // 若文字中已經包含該標籤或已含有多角色 speaker 標籤，則不重複前綴
    if trimmed.contains(tag) || trimmed.starts_with("<|speaker:") {
        trimmed.to_string()
    } else {
        format!("{} {}", tag, trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_models_presets() {
        let chars = get_default_characters();
        assert!(chars.len() >= 8);
        for c in &chars {
            assert!(!c.prompt_tag.is_empty());
        }

        let tones = get_tone_tags();
        assert!(tones.len() >= 15);
        assert!(tones.iter().any(|t| t.category == ToneCategory::Speaker));

        let samples = get_sample_scripts();
        assert!(samples.len() >= 6);
    }

    #[test]
    fn test_format_speech_input() {
        let char_preset = CharacterPreset {
            name: "測試御姐".to_string(),
            description: "測試".to_string(),
            prompt_tag: "[溫柔知性御姐音色]".to_string(),
            voice_id: None,
            recommended_speed: 1.0,
            default_tone: None,
        };

        // 1. 自動套用標籤
        let res = format_speech_input("早安！", &char_preset, true);
        assert_eq!(res, "[溫柔知性御姐音色] 早安！");

        // 2. 已有標籤不重複套用
        let res2 = format_speech_input("[溫柔知性御姐音色] 早安！", &char_preset, true);
        assert_eq!(res2, "[溫柔知性御姐音色] 早安！");

        // 3. 關閉自動套用
        let res3 = format_speech_input("早安！", &char_preset, false);
        assert_eq!(res3, "早安！");

        // 4. 多角色對話標籤不強加角色前綴
        let res4 = format_speech_input("<|speaker:0|> 早安！", &char_preset, true);
        assert_eq!(res4, "<|speaker:0|> 早安！");
    }
}
