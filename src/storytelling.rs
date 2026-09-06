use serde::{Deserialize, Serialize};

/// 故事體裁預設 (源自 fish-audio-s2.1-pro-storytelling presets/storytelling-presets.yaml)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoryPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_tags: Vec<String>,
    pub narrator_tags: Vec<String>,
    pub dialogue_bias: String,
    pub pace: String,
    pub pause_bias: String,
    pub recommended_speed: f32,
}

/// 情緒強度等級 (Level 0 ~ 5)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EmotionIntensityLevel {
    Level0Neutral = 0,    // 平靜/中性 (calm)
    Level1Light = 1,      // 微弱/暗潮 (slightly nervous, softly amused)
    Level2Normal = 2,     // 標準情緒 (nervous, happy, sad, angry)
    Level3Strong = 3,     // 強烈情緒 (very sad, furious, deeply relieved)
    Level4Physical = 4,   // 身體反應 (crying, sobbing, panting, shouting, gasp)
    Level5Cinematic = 5,  // 電影級極致 (screaming in panic, barely able to speak)
}

impl EmotionIntensityLevel {
    pub fn label(&self) -> &'static str {
        match self {
            EmotionIntensityLevel::Level0Neutral => "等級 0：中性平緩 (Neutral)",
            EmotionIntensityLevel::Level1Light => "等級 1：細微暗潮 (Light)",
            EmotionIntensityLevel::Level2Normal => "等級 2：常規情緒 (Normal)",
            EmotionIntensityLevel::Level3Strong => "等級 3：強烈爆發 (Strong)",
            EmotionIntensityLevel::Level4Physical => "等級 4：生理演出 (Physical)",
            EmotionIntensityLevel::Level5Cinematic => "等級 5：極限高潮 (Cinematic)",
        }
    }

    pub fn color(&self) -> [u8; 3] {
        match self {
            EmotionIntensityLevel::Level0Neutral => [156, 163, 175],  // Gray
            EmotionIntensityLevel::Level1Light => [59, 130, 246],    // Blue
            EmotionIntensityLevel::Level2Normal => [16, 185, 129],   // Green
            EmotionIntensityLevel::Level3Strong => [245, 158, 11],   // Amber
            EmotionIntensityLevel::Level4Physical => [239, 68, 68],  // Red
            EmotionIntensityLevel::Level5Cinematic => [168, 85, 247], // Purple
        }
    }
}

/// 導演標籤條目
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DirectorTag {
    pub tag: String,
    pub label: String,
    pub category: String,
    pub intensity: EmotionIntensityLevel,
    pub example: String,
}

/// 故事劇本行
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoryLine {
    pub speaker: String,
    pub is_narrator: bool,
    pub tags: Vec<String>,
    pub text: String,
    pub pause_after_ms: u32,
    pub intensity: EmotionIntensityLevel,
}

/// 故事劇本範本
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoryScriptTemplate {
    pub title: String,
    pub genre: String,
    pub description: String,
    pub scene: String,
    pub energy: f32,
    pub preset_id: String,
    pub lines: Vec<StoryLine>,
}

/// 劇本 QA 檢測問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptQaIssue {
    pub line_index: usize,
    pub severity: ScriptQaSeverity,
    pub message: String,
    pub suggested_fix: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptQaSeverity {
    Info,
    Warning,
    RetryRecommended,
}

impl ScriptQaSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            ScriptQaSeverity::Info => "提示",
            ScriptQaSeverity::Warning => "注意",
            ScriptQaSeverity::RetryRecommended => "建議修正",
        }
    }

    pub fn color(&self) -> [u8; 3] {
        match self {
            ScriptQaSeverity::Info => [59, 130, 246],
            ScriptQaSeverity::Warning => [245, 158, 11],
            ScriptQaSeverity::RetryRecommended => [239, 68, 68],
        }
    }
}

/// 取得所有故事體裁預設 (依據 storytelling-presets.yaml)
pub fn get_story_presets() -> Vec<StoryPreset> {
    vec![
        StoryPreset {
            id: "audiobook-neutral".to_string(),
            name: "標準有聲書 (Audiobook Neutral)".to_string(),
            description: "冷靜、咬字清晰，適合知識普及、歷史紀錄與通用小說朗讀".to_string(),
            base_tags: vec!["calm".to_string(), "clear articulation".to_string()],
            narrator_tags: vec!["calm".to_string(), "clear articulation".to_string()],
            dialogue_bias: "natural".to_string(),
            pace: "medium".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 1.0,
        },
        StoryPreset {
            id: "audiobook-warm".to_string(),
            name: "溫暖有聲書 (Audiobook Warm)".to_string(),
            description: "溫暖說書人口吻，步調沉穩，適合長篇文學與抒情散文".to_string(),
            base_tags: vec!["warm storyteller tone".to_string(), "measured pacing".to_string()],
            narrator_tags: vec!["warm storyteller tone".to_string(), "measured pacing".to_string()],
            dialogue_bias: "expressive".to_string(),
            pace: "medium-slow".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 0.95,
        },
        StoryPreset {
            id: "wuxia".to_string(),
            name: "武俠江湖 (Wuxia / Martial Arts)".to_string(),
            description: "低沉穩重、隱約神秘氣場，對白克制冷峻，營造刀光劍影氛圍".to_string(),
            base_tags: vec!["low voice".to_string(), "measured pacing".to_string(), "slightly mysterious".to_string()],
            narrator_tags: vec!["low voice".to_string(), "measured pacing".to_string(), "slightly mysterious".to_string()],
            dialogue_bias: "restrained".to_string(),
            pace: "medium-slow".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 0.95,
        },
        StoryPreset {
            id: "horror".to_string(),
            name: "恐怖驚悚 (Horror & Thriller)".to_string(),
            description: "低語慢速、神秘壓抑，長停頓營造呼吸不暢的心跳壓迫感".to_string(),
            base_tags: vec!["low voice".to_string(), "speaking slowly".to_string(), "mysterious".to_string()],
            narrator_tags: vec!["low voice".to_string(), "speaking slowly".to_string(), "mysterious".to_string()],
            dialogue_bias: "tense".to_string(),
            pace: "slow".to_string(),
            pause_bias: "high".to_string(),
            recommended_speed: 0.9,
        },
        StoryPreset {
            id: "suspense".to_string(),
            name: "懸疑推理 (Suspense & Mystery)".to_string(),
            description: "冷靜低語，緊繃而克制，適合偵探推理與罪案解謎故事".to_string(),
            base_tags: vec!["calm".to_string(), "low voice".to_string(), "restrained tension".to_string()],
            narrator_tags: vec!["calm".to_string(), "low voice".to_string(), "restrained tension".to_string()],
            dialogue_bias: "calculating".to_string(),
            pace: "medium".to_string(),
            pause_bias: "high".to_string(),
            recommended_speed: 0.95,
        },
        StoryPreset {
            id: "fantasy".to_string(),
            name: "奇幻史詩 (Fantasy & Epic)".to_string(),
            description: "溫暖大器、電影畫面感，適合魔法大陸與史詩戰役敘事".to_string(),
            base_tags: vec!["warm".to_string(), "cinematic".to_string(), "measured pacing".to_string()],
            narrator_tags: vec!["warm".to_string(), "cinematic".to_string(), "measured pacing".to_string()],
            dialogue_bias: "dramatic".to_string(),
            pace: "medium".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 0.98,
        },
        StoryPreset {
            id: "children-story".to_string(),
            name: "童話繪本 (Children's Story)".to_string(),
            description: "親切溫柔、生動活潑，角色表情豐富，深受小朋友喜愛".to_string(),
            base_tags: vec!["warm".to_string(), "playful".to_string(), "expressive".to_string()],
            narrator_tags: vec!["warm".to_string(), "playful".to_string(), "expressive".to_string()],
            dialogue_bias: "playful".to_string(),
            pace: "medium-slow".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 0.92,
        },
        StoryPreset {
            id: "bedtime".to_string(),
            name: "睡前安眠 (Bedtime Story)".to_string(),
            description: "輕柔低語、緩慢催眠，極致療癒與舒適感，伴君安然入夢".to_string(),
            base_tags: vec!["soft voice".to_string(), "warm".to_string(), "speaking slowly".to_string()],
            narrator_tags: vec!["soft voice".to_string(), "warm".to_string(), "speaking slowly".to_string()],
            dialogue_bias: "whispering".to_string(),
            pace: "slow".to_string(),
            pause_bias: "high".to_string(),
            recommended_speed: 0.88,
        },
        StoryPreset {
            id: "podcast".to_string(),
            name: "廣播訪談 (Podcast & Interview)".to_string(),
            description: "親和自然、口語化，彷彿面對面與朋友輕鬆交流".to_string(),
            base_tags: vec!["friendly".to_string(), "conversational".to_string()],
            narrator_tags: vec!["friendly".to_string(), "conversational".to_string()],
            dialogue_bias: "spontaneous".to_string(),
            pace: "medium-fast".to_string(),
            pause_bias: "low".to_string(),
            recommended_speed: 1.05,
        },
        StoryPreset {
            id: "news".to_string(),
            name: "新聞播報 (Professional News)".to_string(),
            description: "專業廣播腔、字正腔圓、平靜權威，適合科技要聞與時事新聞".to_string(),
            base_tags: vec!["professional broadcast tone".to_string(), "calm".to_string(), "clear articulation".to_string()],
            narrator_tags: vec!["professional broadcast tone".to_string(), "calm".to_string(), "clear articulation".to_string()],
            dialogue_bias: "objective".to_string(),
            pace: "medium".to_string(),
            pause_bias: "low".to_string(),
            recommended_speed: 1.0,
        },
        StoryPreset {
            id: "commercial".to_string(),
            name: "商業宣傳 (Commercial / Ads)".to_string(),
            description: "明亮、自信、充滿吸引力與感染力的高能量演繹".to_string(),
            base_tags: vec!["bright".to_string(), "confident and energetic".to_string()],
            narrator_tags: vec!["bright".to_string(), "confident and energetic".to_string()],
            dialogue_bias: "persuasive".to_string(),
            pace: "medium-fast".to_string(),
            pause_bias: "low".to_string(),
            recommended_speed: 1.08,
        },
        StoryPreset {
            id: "elderly-storyteller".to_string(),
            name: "老說書人 (Elderly Storyteller)".to_string(),
            description: "蒼老慈祥、略帶沙啞、緩慢悠長，充滿古老傳奇的歲月感".to_string(),
            base_tags: vec!["elderly storyteller".to_string(), "warm".to_string(), "slightly raspy".to_string(), "slow and measured".to_string()],
            narrator_tags: vec!["elderly storyteller".to_string(), "warm".to_string(), "slightly raspy".to_string(), "slow and measured".to_string()],
            dialogue_bias: "historical".to_string(),
            pace: "slow".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 0.9,
        },
        StoryPreset {
            id: "villain".to_string(),
            name: "冷血反派 (Villain Monologue)".to_string(),
            description: "低沉、危險而冷靜，令人毛骨悚然的幕後黑手支配感".to_string(),
            base_tags: vec!["low voice".to_string(), "dangerously calm".to_string()],
            narrator_tags: vec!["low voice".to_string(), "dangerously calm".to_string()],
            dialogue_bias: "intimidating".to_string(),
            pace: "medium-slow".to_string(),
            pause_bias: "medium".to_string(),
            recommended_speed: 0.95,
        },
    ]
}

/// 取得完整的自然語言導演標籤庫 (依據 tag-library.json 與 tag-library.md)
pub fn get_director_tags() -> Vec<DirectorTag> {
    vec![
        // 生理呼吸與動作 (Physical)
        DirectorTag {
            tag: "[inhale]".to_string(),
            label: "吸氣".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[inhale] 呼……準備好了。".to_string(),
        },
        DirectorTag {
            tag: "[exhale]".to_string(),
            label: "吐氣".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[exhale] 終於結束了。".to_string(),
        },
        DirectorTag {
            tag: "[breathing]".to_string(),
            label: "呼吸聲".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[breathing] 我在聽，你繼續說。".to_string(),
        },
        DirectorTag {
            tag: "[panting]".to_string(),
            label: "急促喘息".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level4Physical,
            example: "[panting] 他……追上來了！".to_string(),
        },
        DirectorTag {
            tag: "[sigh]".to_string(),
            label: "嘆息".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[sigh] 事情怎麼會變成這樣？".to_string(),
        },
        DirectorTag {
            tag: "[gasp]".to_string(),
            label: "倒抽一口氣".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[gasp] 難道是你？！".to_string(),
        },
        DirectorTag {
            tag: "[clears throat]".to_string(),
            label: "清喉嚨".to_string(),
            category: "生理反應".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[clears throat] 各位請注意聽我說。".to_string(),
        },

        // 笑與哭 (Laugh & Cry)
        DirectorTag {
            tag: "[laughing]".to_string(),
            label: "開懷大笑".to_string(),
            category: "笑與哭".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[laughing] 這真是太好笑了！".to_string(),
        },
        DirectorTag {
            tag: "[chuckling]".to_string(),
            label: "忍俊不禁輕笑".to_string(),
            category: "笑與哭".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[chuckling] 你這傢伙，總是這樣。".to_string(),
        },
        DirectorTag {
            tag: "[giggle]".to_string(),
            label: "咯咯嬌笑".to_string(),
            category: "笑與哭".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[giggle] 被你發現了啦！".to_string(),
        },
        DirectorTag {
            tag: "[crying]".to_string(),
            label: "哭泣帶淚".to_string(),
            category: "笑與哭".to_string(),
            intensity: EmotionIntensityLevel::Level4Physical,
            example: "[crying] 求求你不要走……".to_string(),
        },
        DirectorTag {
            tag: "[sobbing]".to_string(),
            label: "抽泣哽咽".to_string(),
            category: "笑與哭".to_string(),
            intensity: EmotionIntensityLevel::Level4Physical,
            example: "[sobbing] 我真的……不知道該怎麼辦……".to_string(),
        },

        // 情感演繹 (Emotion)
        DirectorTag {
            tag: "[calm]".to_string(),
            label: "平靜中性".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level0Neutral,
            example: "[calm] 今天天氣很好。".to_string(),
        },
        DirectorTag {
            tag: "[happy]".to_string(),
            label: "開心喜悅".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[happy] 能見到你真好！".to_string(),
        },
        DirectorTag {
            tag: "[excited]".to_string(),
            label: "興奮澎湃".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[excited] 成功了！我們拿到了！".to_string(),
        },
        DirectorTag {
            tag: "[sad]".to_string(),
            label: "悲傷難過".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[sad] 為什麼最後是這個結局。".to_string(),
        },
        DirectorTag {
            tag: "[very sad]".to_string(),
            label: "極度悲痛".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[very sad] 我再也見不到他了。".to_string(),
        },
        DirectorTag {
            tag: "[angry]".to_string(),
            label: "憤怒生氣".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[angry] 你怎麼敢做出這種事！".to_string(),
        },
        DirectorTag {
            tag: "[barely controlled anger]".to_string(),
            label: "隱忍怒火".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[barely controlled anger] 我再給你最後一次機會。".to_string(),
        },
        DirectorTag {
            tag: "[furious]".to_string(),
            label: "暴怒咆哮".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level4Physical,
            example: "[furious] 夠了！給我滾出去！".to_string(),
        },
        DirectorTag {
            tag: "[scared]".to_string(),
            label: "害怕恐懼".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[scared] 那個陰影……在移動！".to_string(),
        },
        DirectorTag {
            tag: "[nervous]".to_string(),
            label: "緊張焦慮".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[nervous] 輪到我上台了嗎？".to_string(),
        },
        DirectorTag {
            tag: "[tired]".to_string(),
            label: "疲憊乏力".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[tired] 好累，今天先休息吧。".to_string(),
        },
        DirectorTag {
            tag: "[dead tired, end of a very long shift]".to_string(),
            label: "極度虛脫疲倦".to_string(),
            category: "情緒演繹".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[dead tired, end of a very long shift] 我連一根手指頭都動不了了……".to_string(),
        },

        // 語氣風格 (Delivery)
        DirectorTag {
            tag: "[soft voice]".to_string(),
            label: "輕柔聲線".to_string(),
            category: "演繹方式".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[soft voice] 閉上眼，好好睡一覺。".to_string(),
        },
        DirectorTag {
            tag: "[whispering]".to_string(),
            label: "耳語低語".to_string(),
            category: "演繹方式".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[whispering] 噓，別出聲，巡邏隊過來了。".to_string(),
        },
        DirectorTag {
            tag: "[low voice]".to_string(),
            label: "低沉聲線".to_string(),
            category: "演繹方式".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[low voice] 這件事，到此為止。".to_string(),
        },
        DirectorTag {
            tag: "[professional broadcast tone]".to_string(),
            label: "專業播音腔".to_string(),
            category: "演繹方式".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[professional broadcast tone] 晚間新聞，現在為您播報。".to_string(),
        },
        DirectorTag {
            tag: "[loud voice]".to_string(),
            label: "宏亮高聲".to_string(),
            category: "演繹方式".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[loud voice] 全體注意，立正！".to_string(),
        },
        DirectorTag {
            tag: "[shouting]".to_string(),
            label: "大聲呼喊".to_string(),
            category: "演繹方式".to_string(),
            intensity: EmotionIntensityLevel::Level4Physical,
            example: "[shouting] 小心前面的懸崖！".to_string(),
        },

        // 語速節奏 (Pace)
        DirectorTag {
            tag: "[speaking slowly]".to_string(),
            label: "緩慢深思".to_string(),
            category: "語速節奏".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "[speaking slowly] 每一代人，都有屬於自己的使命。".to_string(),
        },
        DirectorTag {
            tag: "[speaking very slowly and deliberately]".to_string(),
            label: "極慢字句斟酌".to_string(),
            category: "語速節奏".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[speaking very slowly and deliberately] 你……確定……要這麼做嗎？".to_string(),
        },
        DirectorTag {
            tag: "[speaking quickly]".to_string(),
            label: "快速急促".to_string(),
            category: "語速節奏".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[speaking quickly] 時間不多了，拿好裝備立刻出發！".to_string(),
        },

        // 停頓與重音 (Pause & Emphasis)
        DirectorTag {
            tag: "[short pause]".to_string(),
            label: "短暫停頓".to_string(),
            category: "停頓重音".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "他回過頭 [short pause] 看了我一眼。".to_string(),
        },
        DirectorTag {
            tag: "[pause]".to_string(),
            label: "標準停頓".to_string(),
            category: "停頓重音".to_string(),
            intensity: EmotionIntensityLevel::Level1Light,
            example: "真相只有一個 [pause] 那就是你。".to_string(),
        },
        DirectorTag {
            tag: "[long pause]".to_string(),
            label: "長懸念停頓".to_string(),
            category: "停頓重音".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "門把緩緩轉動 [long pause] 隨後是一聲輕響。".to_string(),
        },
        DirectorTag {
            tag: "[emphasis]".to_string(),
            label: "強調焦點".to_string(),
            category: "停頓重音".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "這 [emphasis] 絕不可能 是巧合！".to_string(),
        },

        // 電影複合導演指令 (Cinematic / Director Compound)
        DirectorTag {
            tag: "[voice trembling, trying not to cry]".to_string(),
            label: "隱忍顫抖哭腔".to_string(),
            category: "電影複合".to_string(),
            intensity: EmotionIntensityLevel::Level5Cinematic,
            example: "[voice trembling, trying not to cry] 今天……原本應該是個好日子的。".to_string(),
        },
        DirectorTag {
            tag: "[low voice, dangerously calm]".to_string(),
            label: "低沉危險平靜".to_string(),
            category: "電影複合".to_string(),
            intensity: EmotionIntensityLevel::Level5Cinematic,
            example: "[low voice, dangerously calm] 你以為，你還能活著離開這裡？".to_string(),
        },
        DirectorTag {
            tag: "[warm storyteller tone, measured pacing]".to_string(),
            label: "溫厚說書節奏".to_string(),
            category: "電影複合".to_string(),
            intensity: EmotionIntensityLevel::Level2Normal,
            example: "[warm storyteller tone, measured pacing] 很多年前，在遙遠的北方森林裡。".to_string(),
        },
        DirectorTag {
            tag: "[elderly storyteller, warm, slightly raspy, slow and measured]".to_string(),
            label: "滄桑老說書人".to_string(),
            category: "電影複合".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[elderly storyteller, warm, slightly raspy, slow and measured] 那段舊事，已經很久沒人提起了……".to_string(),
        },
        DirectorTag {
            tag: "[bright, confident and energetic]".to_string(),
            label: "明亮自信高能".to_string(),
            category: "電影複合".to_string(),
            intensity: EmotionIntensityLevel::Level3Strong,
            example: "[bright, confident and energetic] 探索全新未來，今天就開始您的精彩旅程！".to_string(),
        },
    ]
}

/// 取得豐富的內建故事範本集 (涵蓋武俠、恐怖、奇幻、童話、睡前、懸疑、商業)
pub fn get_story_script_templates() -> Vec<StoryScriptTemplate> {
    vec![
        StoryScriptTemplate {
            title: "武俠夜探《古道殘陽》".to_string(),
            genre: "武俠懸疑".to_string(),
            description: "夜色沉沉的古道之上，隱世劍客與冷血追殺者的命運相遇".to_string(),
            scene: "深夜荒山古道".to_string(),
            energy: 0.4,
            preset_id: "wuxia".to_string(),
            lines: vec![
                StoryLine {
                    speaker: "旁白".to_string(),
                    is_narrator: true,
                    tags: vec!["low voice".to_string(), "speaking slowly".to_string(), "mysterious".to_string()],
                    text: "夜色沉沉，古道荒草叢生，四下沒有半點人聲。".to_string(),
                    pause_after_ms: 400,
                    intensity: EmotionIntensityLevel::Level2Normal,
                },
                StoryLine {
                    speaker: "冷面刺客".to_string(),
                    is_narrator: false,
                    tags: vec!["low voice, dangerously calm".to_string()],
                    text: "交出玄鐵令，我留你全屍。".to_string(),
                    pause_after_ms: 300,
                    intensity: EmotionIntensityLevel::Level3Strong,
                },
                StoryLine {
                    speaker: "隱世劍客".to_string(),
                    is_narrator: false,
                    tags: vec!["sigh".to_string(), "calm".to_string()],
                    text: "退隱十年，終究還是躲不過這場血雨腥風。".to_string(),
                    pause_after_ms: 350,
                    intensity: EmotionIntensityLevel::Level2Normal,
                },
                StoryLine {
                    speaker: "旁白".to_string(),
                    is_narrator: true,
                    tags: vec!["whispering".to_string(), "short pause".to_string()],
                    text: "寒芒乍現，殘葉落處，風止。".to_string(),
                    pause_after_ms: 500,
                    intensity: EmotionIntensityLevel::Level3Strong,
                },
            ],
        },
        StoryScriptTemplate {
            title: "恐怖驚悚《午夜鐘聲》".to_string(),
            genre: "恐怖靈異".to_string(),
            description: "廢棄老洋房裡迴響的鐘聲，壓抑窒息的探索體驗".to_string(),
            scene: "廢棄荒廢古宅".to_string(),
            energy: 0.7,
            preset_id: "horror".to_string(),
            lines: vec![
                StoryLine {
                    speaker: "旁白".to_string(),
                    is_narrator: true,
                    tags: vec!["low voice".to_string(), "speaking slowly".to_string(), "mysterious".to_string()],
                    text: "整棟洋房空無一人，但空氣中卻飄著淡淡的潮濕霉味。".to_string(),
                    pause_after_ms: 450,
                    intensity: EmotionIntensityLevel::Level2Normal,
                },
                StoryLine {
                    speaker: "探險者".to_string(),
                    is_narrator: false,
                    tags: vec!["whispering".to_string(), "nervous".to_string()],
                    text: "喂……有人在那裡嗎？不要開這種玩笑……".to_string(),
                    pause_after_ms: 350,
                    intensity: EmotionIntensityLevel::Level2Normal,
                },
                StoryLine {
                    speaker: "旁白".to_string(),
                    is_narrator: true,
                    tags: vec!["gasp".to_string(), "long pause".to_string()],
                    text: "咚……午夜十二點的鐘聲，毫無預警地敲響了第一聲。".to_string(),
                    pause_after_ms: 600,
                    intensity: EmotionIntensityLevel::Level4Physical,
                },
                StoryLine {
                    speaker: "未知幽語".to_string(),
                    is_narrator: false,
                    tags: vec!["whispering".to_string(), "soft voice".to_string()],
                    text: "你終於……來陪我了……".to_string(),
                    pause_after_ms: 500,
                    intensity: EmotionIntensityLevel::Level5Cinematic,
                },
            ],
        },
        StoryScriptTemplate {
            title: "睡前童話《星光森林的小狐狸》".to_string(),
            genre: "睡前安眠".to_string(),
            description: "輕柔舒緩的童話語調，適合哄睡與放鬆入眠".to_string(),
            scene: "微光閃爍的月夜森林".to_string(),
            energy: 0.2,
            preset_id: "bedtime".to_string(),
            lines: vec![
                StoryLine {
                    speaker: "說書人".to_string(),
                    is_narrator: true,
                    tags: vec!["soft voice".to_string(), "warm".to_string(), "speaking slowly".to_string()],
                    text: "月亮升起來了，柔和的銀光灑在整片安靜的森林上。".to_string(),
                    pause_after_ms: 400,
                    intensity: EmotionIntensityLevel::Level1Light,
                },
                StoryLine {
                    speaker: "小狐狸".to_string(),
                    is_narrator: false,
                    tags: vec!["soft voice".to_string(), "sigh".to_string()],
                    text: "今天走了一整天，星星看起來好溫暖呀。".to_string(),
                    pause_after_ms: 300,
                    intensity: EmotionIntensityLevel::Level1Light,
                },
                StoryLine {
                    speaker: "說書人".to_string(),
                    is_narrator: true,
                    tags: vec!["warm storyteller tone, measured pacing".to_string()],
                    text: "小狐狸蜷縮在厚厚的苔蘚上，慢慢闔上了眼睛。晚安，做個好夢。".to_string(),
                    pause_after_ms: 600,
                    intensity: EmotionIntensityLevel::Level0Neutral,
                },
            ],
        },
        StoryScriptTemplate {
            title: "奇幻史詩《巨龍王座的誓言》".to_string(),
            genre: "奇幻史詩".to_string(),
            description: "大氣澎湃的傳奇篇章，英勇騎士面對遠古巨龍的對抗".to_string(),
            scene: "龍骨荒原懸崖之巔".to_string(),
            energy: 0.85,
            preset_id: "fantasy".to_string(),
            lines: vec![
                StoryLine {
                    speaker: "史詩旁白".to_string(),
                    is_narrator: true,
                    tags: vec!["warm".to_string(), "cinematic".to_string(), "measured pacing".to_string()],
                    text: "一千年前沉睡的黑翼，在今日的狂風中再度遮蔽了蒼穹。".to_string(),
                    pause_after_ms: 450,
                    intensity: EmotionIntensityLevel::Level2Normal,
                },
                StoryLine {
                    speaker: "遠古龍王".to_string(),
                    is_narrator: false,
                    tags: vec!["low voice".to_string(), "loud voice".to_string()],
                    text: "凡人，汝之血肉，何以抵禦烈焰？".to_string(),
                    pause_after_ms: 400,
                    intensity: EmotionIntensityLevel::Level3Strong,
                },
                StoryLine {
                    speaker: "騎士團長".to_string(),
                    is_narrator: false,
                    tags: vec!["excited".to_string(), "emphasis".to_string()],
                    text: "我們身後就是王國最後的希望，拔劍！為了誓言！".to_string(),
                    pause_after_ms: 500,
                    intensity: EmotionIntensityLevel::Level4Physical,
                },
            ],
        },
    ]
}

/// 依據文本與預設自動導演 (Auto-Director)：分段、判斷角色/旁白並加入合適的演出標籤
pub fn auto_direct_story(text: &str, preset: &StoryPreset) -> Vec<StoryLine> {
    let mut result = Vec::new();
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return result;
    }

    // 分行處理
    for raw_line in cleaned.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // 判斷是否為對白 (含有引號「」或 "")
        let is_dialogue = (line.starts_with('「') && line.ends_with('」'))
            || (line.starts_with('“') && line.ends_with('”'))
            || (line.starts_with('"') && line.ends_with('"'));

        let mut tags = Vec::new();
        let speaker;
        let is_narrator;
        let intensity;
        let mut pause_ms = 350;

        if is_dialogue {
            speaker = "對白角色".to_string();
            is_narrator = false;

            // 依標點或情緒詞微調對白標籤
            if line.contains('！') || line.contains('!') {
                tags.push("excited".to_string());
                intensity = EmotionIntensityLevel::Level3Strong;
            } else if line.contains('？') || line.contains('?') {
                tags.push("nervous".to_string());
                intensity = EmotionIntensityLevel::Level2Normal;
            } else if line.contains("……") || line.contains("...") {
                tags.push("soft voice".to_string());
                tags.push("short pause".to_string());
                pause_ms = 500;
                intensity = EmotionIntensityLevel::Level2Normal;
            } else {
                tags.push("calm".to_string());
                intensity = EmotionIntensityLevel::Level1Light;
            }
        } else {
            speaker = "旁白".to_string();
            is_narrator = true;
            // 旁白套用預設的 narrator_tags
            tags.extend(preset.narrator_tags.clone());

            if preset.pause_bias == "high" {
                pause_ms = 500;
            } else if preset.pause_bias == "low" {
                pause_ms = 250;
            }
            intensity = EmotionIntensityLevel::Level2Normal;
        }

        result.push(StoryLine {
            speaker,
            is_narrator,
            tags,
            text: line.to_string(),
            pause_after_ms: pause_ms,
            intensity,
        });
    }

    result
}

/// 劇本品質 QA 檢測 (依據 fish-audio-s2.1-pro-storytelling references/qa-and-retry.md)
pub fn qa_check_story_script(lines: &[StoryLine]) -> Vec<ScriptQaIssue> {
    let mut issues = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let text_chars = line.text.chars().count();

        // 1. 單段過長檢測 (> 250 字建議切分以保證 TTS 品質)
        if text_chars > 250 {
            issues.push(ScriptQaIssue {
                line_index: idx,
                severity: ScriptQaSeverity::Warning,
                message: format!("第 {} 行文字過長 ({} 字)，可能導致 TTS 語速失控或情緒漂移", idx + 1, text_chars),
                suggested_fix: "建議拆分為 1~4 句的獨立小段落，以獲得更精確表演效果".to_string(),
            });
        }

        // 2. 標籤過多堆疊檢測 (> 4 個標籤可能互相競爭衝突)
        if line.tags.len() > 4 {
            issues.push(ScriptQaIssue {
                line_index: idx,
                severity: ScriptQaSeverity::RetryRecommended,
                message: format!("第 {} 行標籤堆疊過多 ({} 個)，可能造成模型理解混亂", idx + 1, line.tags.len()),
                suggested_fix: "保留 1 個生理反應 + 1 個核心情緒 + 1 個演繹風格即可，減少競爭標籤".to_string(),
            });
        }

        // 3. 連續過多驚嘆號檢測
        let exclamation_count = line.text.chars().filter(|c| *c == '！' || *c == '!').count();
        if exclamation_count >= 3 {
            issues.push(ScriptQaIssue {
                line_index: idx,
                severity: ScriptQaSeverity::Warning,
                message: format!("第 {} 行含有過多驚嘆號 ({} 個)，聲音可能過於破音或過度激動", idx + 1, exclamation_count),
                suggested_fix: "將部分驚嘆號改為句號或省略號，透過 [excited] 或 [loud voice] 標籤來呈現氣勢".to_string(),
            });
        }

        // 4. 空文本檢測
        if line.text.trim().is_empty() {
            issues.push(ScriptQaIssue {
                line_index: idx,
                severity: ScriptQaSeverity::RetryRecommended,
                message: format!("第 {} 行台詞內容為空", idx + 1),
                suggested_fix: "請填入台詞文字或移除此空白段落".to_string(),
            });
        }
    }

    issues
}

/// 將 StoryLine 格式化為發送給 Fish Audio TTS 的文字 (帶導演標籤)
pub fn format_story_line_tts(line: &StoryLine) -> String {
    let trimmed = line.text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut prefix = String::new();
    for t in &line.tags {
        let tag_clean = t.trim();
        if tag_clean.starts_with('[') && tag_clean.ends_with(']') {
            prefix.push_str(tag_clean);
        } else {
            prefix.push('[');
            prefix.push_str(tag_clean);
            prefix.push(']');
        }
    }

    if prefix.is_empty() {
        trimmed.to_string()
    } else {
        format!("{} {}", prefix, trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_story_presets() {
        let presets = get_story_presets();
        assert!(presets.len() >= 10);
        let wuxia = presets.iter().find(|p| p.id == "wuxia").expect("應有武俠預設");
        assert!(wuxia.narrator_tags.contains(&"low voice".to_string()));
    }

    #[test]
    fn test_get_director_tags() {
        let tags = get_director_tags();
        assert!(tags.len() >= 25);
        assert!(tags.iter().any(|t| t.tag == "[whispering]"));
        assert!(tags.iter().any(|t| t.tag == "[crying]"));
        assert!(tags.iter().any(|t| t.intensity == EmotionIntensityLevel::Level5Cinematic));
    }

    #[test]
    fn test_auto_direct_story() {
        let presets = get_story_presets();
        let wuxia = &presets[2];
        let script_text = "夜幕低垂，寒風刺骨。\n「站住！交出解藥！」\n「你休想……」";
        let lines = auto_direct_story(script_text, wuxia);

        assert_eq!(lines.len(), 3);
        assert!(lines[0].is_narrator);
        assert!(!lines[1].is_narrator);
        assert!(lines[1].tags.contains(&"excited".to_string()));
        assert!(lines[2].tags.contains(&"soft voice".to_string()));
    }

    #[test]
    fn test_qa_check_story_script() {
        let long_text = "測試".repeat(150);
        let test_lines = vec![
            StoryLine {
                speaker: "旁白".to_string(),
                is_narrator: true,
                tags: vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string()],
                text: "太棒了！！！真的太棒了！！！".to_string(),
                pause_after_ms: 300,
                intensity: EmotionIntensityLevel::Level3Strong,
            },
            StoryLine {
                speaker: "長文".to_string(),
                is_narrator: false,
                tags: vec!["calm".to_string()],
                text: long_text,
                pause_after_ms: 300,
                intensity: EmotionIntensityLevel::Level1Light,
            },
        ];

        let issues = qa_check_story_script(&test_lines);
        assert!(issues.iter().any(|i| i.message.contains("標籤堆疊過多")));
        assert!(issues.iter().any(|i| i.message.contains("過多驚嘆號")));
        assert!(issues.iter().any(|i| i.message.contains("文字過長")));
    }

    #[test]
    fn test_format_story_line_tts() {
        let line = StoryLine {
            speaker: "主角".to_string(),
            is_narrator: false,
            tags: vec!["whispering".to_string(), "[sad]".to_string()],
            text: "天黑了。".to_string(),
            pause_after_ms: 300,
            intensity: EmotionIntensityLevel::Level2Normal,
        };
        let formatted = format_story_line_tts(&line);
        assert_eq!(formatted, "[whispering][sad] 天黑了。");
    }
}
