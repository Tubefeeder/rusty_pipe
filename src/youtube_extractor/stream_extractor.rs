use super::super::downloader_trait::Downloader;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

use super::super::utils::utils::*;
use super::itag_item::ItagType;
use crate::youtube_extractor::error::ParsingError;
use crate::youtube_extractor::search_extractor::YTSearchItem;
use lazy_static::lazy_static;

const FORMATS: &str = "formats";
const ADAPTIVE_FORMATS: &str = "adaptiveFormats";
const HTTPS: &str = "https:";
const DECRYPTION_FUNC_NAME: &str = "decrypt";

lazy_static! {
    static ref REGEXES: Vec<&'static str>=vec![
        "(?:\\b|[^a-zA-Z0-9$])([a-zA-Z0-9$]{2})\\s*=\\s*function\\(\\s*a\\s*\\)\\s*\\{\\s*a\\s*=\\s*a\\.split\\(\\s*\"\"\\s*\\)",
        "([\\w$]+)\\s*=\\s*function\\((\\w+)\\)\\{\\s*\\2=\\s*\\2\\.split\\(\"\"\\)\\s*;",
        "\\b([\\w$]{2})\\s*=\\s*function\\((\\w+)\\)\\{\\s*\\2=\\s*\\2\\.split\\(\"\"\\)\\s*;",
        "yt\\.akamaized\\.net/\\)\\s*\\|\\|\\s*.*?\\s*c\\s*&&\\s*d\\.set\\([^,]+\\s*,\\s*(:encodeURIComponent\\s*\\()([a-zA-Z0-9$]+)\\(",
        "\\bc\\s*&&\\s*d\\.set\\([^,]+\\s*,\\s*(:encodeURIComponent\\s*\\()([a-zA-Z0-9$]+)\\("
    ];
}

pub const HARDCODED_CLIENT_VERSION: &str = "2.20200214.04.00";

#[derive(Clone, PartialEq)]
pub struct YTStreamExtractor<D: Downloader> {
    doc: String,
    player_response: Map<String, Value>,
    player_code: String,
    video_id: String,

    initial_data: Value,
    primary_info_renderer: Value,
    secondary_info_renderer: Value,
    downloader: D,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamItem {
    pub url: Option<String>,
    pub itag: u32,
    pub approx_duration_ms: Option<String>,
    pub audio_channels: Option<u32>,
    pub audio_quality: Option<String>,
    pub audio_sample_rate: Option<String>,
    pub average_bitrate: Option<u32>,
    pub bitrate: u32,
    pub content_length: Option<String>,
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub quality: String,
    pub quality_label: Option<String>,
    pub last_modified: String,
    pub mime_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    pub width: u64,
    pub height: u64,
}

impl<D: Downloader> YTStreamExtractor<D> {
    pub async fn new(downloader: D, video_id: &str) -> Result<Self, ParsingError> {
        use futures::try_join;
        let url = format!(
            "https://www.youtube.com/watch?v={}&disable_polymer=1",
            video_id
        );

        let doc = downloader.download(&url);
        let inital_ajax_json = Self::initial_ajax_json(&downloader, &url).await?;
        let initial_data = YTStreamExtractor::<D>::initial_data(&inital_ajax_json);
        let (doc, initial_data) = try_join!(doc, initial_data)?;

        let initial_response = Self::player_response_from_initial_ajax(&inital_ajax_json);

        if initial_data.1 {
            return Err(ParsingError::AgeRestricted);
        }

        let initial_data = initial_data.0;
        let primary_info_renderer = YTStreamExtractor::<D>::primary_info_renderer(&initial_data)?;
        let secondary_info_renderer =
            YTStreamExtractor::<D>::secondary_info_renderer(&initial_data)?;
        if let Some(response) = initial_response {
            Ok(YTStreamExtractor {
                player_response: response,
                downloader,
                player_code: "".to_owned(),
                initial_data,
                primary_info_renderer,
                secondary_info_renderer,
                doc: String::from(doc),
                video_id: String::from(video_id),
            })
        } else {
            // OLD METHOD
            let player_config = YTStreamExtractor::<D>::player_config(&doc)
                .ok_or("cannot get player_config".to_string())?;

            let player_args = YTStreamExtractor::<D>::player_args(&player_config)
                .ok_or("cannot get player args".to_string())?;

            let player_response = YTStreamExtractor::<D>::player_response(&player_args)
                .ok_or("cannot get player response".to_string())?;
            let player_url = YTStreamExtractor::<D>::player_url(&player_config)
                .ok_or("Cant get player url".to_owned())?;
            let player_code = YTStreamExtractor::<D>::player_code(&downloader, &player_url).await?;
            Ok(YTStreamExtractor {
                player_response,
                downloader,
                player_code,
                initial_data,
                primary_info_renderer,
                secondary_info_renderer,
                doc: String::from(doc),
                video_id: String::from(video_id),
            })
        }
    }

    fn itags(
        downloader: &D,
        streaming_data_key: &str,
        itag_type_wanted: ItagType,
        player_response: &Map<String, Value>,
        decryption_code: &str,
    ) -> Result<HashMap<String, StreamItem>, ParsingError> {
        let mut url_and_itags = HashMap::new();
        let streaming_data = player_response.get("streamingData").unwrap_or(&Value::Null);
        if let Value::Object(streaming_data) = streaming_data {
            if let Value::Array(formats) = streaming_data
                .get(streaming_data_key)
                .unwrap_or(&Value::Null)
            {
                for format_data in formats {
                    if let Value::Object(format_data_obj) = format_data {
                        let stream_url = match format_data_obj.get("url").unwrap_or(&Value::Null) {
                            Value::String(url) => String::from(url),
                            _ => {
                                let cipherstr = {
                                    if let Value::String(cip) = format_data_obj
                                        .get("cipher")
                                        .or(format_data_obj.get("signatureCipher"))
                                        .unwrap_or(&Value::Null)
                                    {
                                        cip.clone()
                                    } else {
                                        String::default()
                                    }
                                };
                                let cipher = compat_parse_map(&cipherstr);
                                format!(
                                    "{}&{}={}",
                                    cipher.get("url").unwrap_or(&String::default()),
                                    cipher.get("sp").unwrap_or(&String::default()),
                                    &YTStreamExtractor::<D>::decrypt_signature(
                                        downloader,
                                        cipher.get("s").unwrap_or(&"".to_owned()),
                                        decryption_code
                                    )
                                )
                            }
                        };
                        match serde_json::from_value::<StreamItem>(format_data.clone()) {
                            Ok(stream_item) => match itag_type_wanted {
                                ItagType::VideoOnly => {
                                    if stream_item.audio_quality.is_none() {
                                        url_and_itags.insert(stream_url, stream_item);
                                    }
                                }
                                ItagType::Audio => {
                                    if stream_item.height.is_none() {
                                        url_and_itags.insert(stream_url, stream_item);
                                    }
                                }
                                _ => {
                                    url_and_itags.insert(stream_url, stream_item);
                                }
                            },
                            Err(_) => {}
                        }
                    }
                }
            } else {
                return Ok(url_and_itags);
            }
        } else {
            return Err(ParsingError::ParsingError {
                cause: "Streaming data not found in player response".to_string(),
            });
        }

        Ok(url_and_itags)
    }

    pub async fn player_code(downloader: &D, player_url: &str) -> Result<String, ParsingError> {
        let player_url = {
            if player_url.starts_with("http://") || player_url.starts_with("https://") {
                player_url.to_string()
            } else {
                format!("https://youtube.com{}", player_url)
            }
        };
        let player_code = downloader.download(&player_url).await?;
        let player_code = YTStreamExtractor::<D>::load_decryption_code(&player_code)?;
        Ok(player_code)
    }

    fn decrypt_signature(downloader: &D, encrypted_sig: &str, decryption_code: &str) -> String {
        println!("encrypted_sig: {:#?}", encrypted_sig);
        println!("decryption_code {:#?}", decryption_code);

        let script = format!("{};decrypt(\"{}\")", decryption_code, encrypted_sig);
        let res = downloader.eval_js(&script);

        let result = res.unwrap_or_default();

        result
    }

    fn player_config(page_html: &str) -> Option<Map<String, Value>> {
        let pattern = regex::Regex::new(r"ytplayer.config\s*=\s*(\{.*?\});").ok()?;
        let grp = pattern.captures(page_html)?;
        let yt_player_config_raw = grp.get(1)?.as_str();
        let v: Value = serde_json::from_str(yt_player_config_raw).ok()?;
        if let Value::Object(val) = v {
            return Some(val);
        }
        None
    }

    fn player_args(player_config: &Map<String, Value>) -> Option<Map<String, Value>> {
        let args = player_config.get("args")?;
        if let Value::Object(args) = args {
            return Some(args.to_owned());
        }
        None
    }

    fn fix_player_url(url: &str) -> String {
        let mut player_url = url.to_string();
        if player_url.starts_with("//") {
            player_url = HTTPS.to_owned() + &player_url;
        } else if player_url.starts_with("/") {
            player_url = HTTPS.to_owned() + "//www.youtube.com" + &player_url;
        }
        player_url.to_string()
    }

    fn player_url(player_config: &Map<String, Value>) -> Option<String> {
        let yt_assets = player_config.get("assets")?.as_object()?;
        let mut player_url = yt_assets.get("js")?.as_str()?.to_owned();
        player_url = Self::fix_player_url(&player_url);
        Some(player_url)
    }

    fn player_response_from_initial_ajax(inital_ajax_json: &Value) -> Option<Map<String, Value>> {
        let resp = inital_ajax_json.get(2)?.get("playerResponse")?;
        if let None = resp.get("streamingData") {
            None
        } else {
            Some(resp.as_object()?.clone())
        }
    }

    fn player_response(player_args: &Map<String, Value>) -> Option<Map<String, Value>> {
        let player_response_str = player_args.get("player_response")?.as_str()?;
        let player_response: Value = serde_json::from_str(player_response_str).ok()?;
        Some(player_response.as_object()?.to_owned())
    }

    async fn initial_ajax_json(downloader: &D, url: &str) -> Result<Value, ParsingError> {
        let mut headers = HashMap::new();
        headers.insert("X-YouTube-Client-Name".to_string(), "1".to_string());
        headers.insert(
            "X-YouTube-Client-Version".to_string(),
            HARDCODED_CLIENT_VERSION.to_string(),
        );
        let url = format!("{}&pbj=1", url);
        let data = downloader.download_with_header(&url, headers).await?;
        let initial_ajax_json: Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        Ok(initial_ajax_json)
    }

    async fn initial_data(initial_ajax_json: &Value) -> Result<(Value, bool), ParsingError> {
        let initial_ajax_json = initial_ajax_json
            .as_array()
            .ok_or("inital ajax json not array")?;
        if let Some(initial_data) = initial_ajax_json
            .get(2)
            .ok_or("inital ajax 2 not found")?
            .as_object()
        {
            if let Some(response) = initial_data.get("response") {
                Ok((response.clone(), true))
            } else {
                if let Some(initial_data) = initial_ajax_json
                    .get(3)
                    .ok_or("initial ajax 2 not found")?
                    .as_object()
                {
                    if let Some(response) = initial_data.get("response") {
                        Ok((response.clone(), false))
                    } else {
                        Err(ParsingError::ParsingError {
                            cause: "Cannot get initial data".to_string(),
                        })
                    }
                } else {
                    Err(ParsingError::ParsingError {
                        cause: "initial ajax doesnt have index 3".to_string(),
                    })
                }
            }
        } else {
            Err(ParsingError::ParsingError {
                cause: "initial ajax doesnt have index 2".to_string(),
            })
        }
    }

    fn primary_info_renderer(inital_data: &Value) -> Result<Value, ParsingError> {
        let contents = inital_data
            .get("contents")
            .and_then(|content| content.get("twoColumnWatchNextResults"))
            .and_then(|content| content.get("results"))
            .and_then(|content| content.get("results"))
            .and_then(|content| content.get("contents"))
            .and_then(|contents| contents.as_array())
            .ok_or(ParsingError::ParsingError {
                cause: "cant get contents".to_string(),
            })?;

        for content in contents {
            if let Some(info) = content.get("videoPrimaryInfoRenderer") {
                return Ok(info.clone());
            }
        }
        Err(ParsingError::ParsingError {
            cause: "could not get primary info renderer".to_string(),
        })
    }
    fn secondary_info_renderer(inital_data: &Value) -> Result<Value, ParsingError> {
        let contents = inital_data
            .get("contents")
            .and_then(|content| content.get("twoColumnWatchNextResults"))
            .and_then(|content| content.get("results"))
            .and_then(|content| content.get("results"))
            .and_then(|content| content.get("contents"))
            .and_then(|contents| contents.as_array())
            .ok_or(ParsingError::ParsingError {
                cause: "cant get contents".to_string(),
            })?;

        for content in contents {
            if let Some(info) = content.get("videoSecondaryInfoRenderer") {
                return Ok(info.clone());
            }
        }
        Err(ParsingError::ParsingError {
            cause: "could not get primary info renderer".to_string(),
        })
    }

    fn load_decryption_code(player_code: &str) -> Result<String, ParsingError> {
        let decryption_func_name = YTStreamExtractor::<D>::decryption_func_name(player_code)
            .ok_or(ParsingError::parsing_error_from_str(
                "Cant find decryption function",
            ))?;

        let function_pattern = format!(
            "({}=function\\([a-zA-Z0-9_]+\\)\\{{.+?\\}})",
            decryption_func_name.replace("$", "\\$")
        );

        let decryption_func = format!(
            "var {};",
            YTStreamExtractor::<D>::match_group1(&function_pattern, &player_code)?
        );

        let helper_object_name = YTStreamExtractor::<D>::match_group1(
            ";([A-Za-z0-9_\\$]{2})\\...\\(",
            &decryption_func,
        )?;

        let helper_pattern = format!(
            "(var {}=\\{{.+?\\}}\\}};)",
            helper_object_name.replace("$", "\\$")
        );

        let helper_object =
            YTStreamExtractor::<D>::match_group1(&helper_pattern, &player_code.replace("\n", ""))?;

        let caller_function = format!(
            "function {}(a){{return {}(a);}}",
            DECRYPTION_FUNC_NAME, decryption_func_name
        );

        Ok(format!(
            "{}{}{}",
            helper_object, decryption_func, caller_function
        ))
    }

    fn decryption_func_name(player_code: &str) -> Option<String> {
        for reg in REGEXES.iter() {
            let rege = fancy_regex::Regex::new(reg).ok()?;
            let capture = rege.captures(player_code).unwrap();
            if let Some(capture) = capture {
                return capture.get(1).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    fn match_group1(reg: &str, text: &str) -> Result<String, ParsingError> {
        let rege = fancy_regex::Regex::new(reg).expect("Regex is wrong");
        let capture = rege.captures(text).map_err(|e| e.to_string())?;
        if let Some(capture) = capture {
            return capture
                .get(1)
                .map(|m| m.as_str().to_string())
                .ok_or(ParsingError::parsing_error_from_str("group 1 not found"));
        }
        Err(ParsingError::parsing_error_from_str("regex not match"))
    }
}

impl<D: Downloader> YTStreamExtractor<D> {
    pub fn name(&self) -> Result<String, ParsingError> {
        let mut title = String::new();
        if let Some(title_ob) = self.primary_info_renderer.get("title") {
            let title_ob = text_from_object(title_ob, false)?;
            if let Some(title_o) = title_ob {
                title = title_o;
            }
        }
        if title.is_empty() {
            if let Some(t) = self
                .player_response
                .get("videoDetails")
                .and_then(|t| t.get("title"))
                .and_then(|t| t.as_str())
            {
                title = t.to_string();
            }
        }
        if title.is_empty() {
            Err(ParsingError::parsing_error_from_str("Cant get title"))
        } else {
            Ok(title)
        }
    }

    pub fn upload_date(&self) -> Result<chrono::NaiveDate, ParsingError> {
        self.textual_upload_date()
            .map(|s| chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d"))?
            .map_err(|_e| ParsingError::parsing_error_from_str("Cannot parse date"))
    }

    pub fn textual_upload_date(&self) -> Result<String, ParsingError> {
        let micro = self
            .player_response
            .get("microformat")
            .ok_or(ParsingError::parsing_error_from_str(
                "Cannot get upload date (microformat)",
            ))?
            .get("playerMicroformatRenderer")
            .ok_or(ParsingError::parsing_error_from_str(
                "Cannot get upload date (playerMicroformatRenderer)",
            ))?;

        if let Some(date) = micro.get("uploadDate") {
            return date
                .as_str()
                .ok_or(ParsingError::parsing_error_from_str(
                    "Cannot get upload date (uploadDate)",
                ))
                .map(|s| s.to_owned());
        } else if let Some(date) = micro.get("publishDate") {
            return date
                .as_str()
                .ok_or(ParsingError::parsing_error_from_str(
                    "Cannot get upload date (publishDate)",
                ))
                .map(|s| s.to_owned());
        } else {
            // Stream is a lifestream
            let life_details =
                micro
                    .get("liveBroadcastDetails")
                    .ok_or(ParsingError::parsing_error_from_str(
                        "Cannot get upload date (liveBroadcastDetails)",
                    ))?;
            if let Some(date) = life_details.get("endTimestamp") {
                return date
                    .as_str()
                    .ok_or(ParsingError::parsing_error_from_str(
                        "Cannot get upload date (endTimestamp)",
                    ))
                    .map(|s| s.to_owned());
            } else if let Some(date) = life_details.get("startTimestamp") {
                return date
                    .as_str()
                    .ok_or(ParsingError::parsing_error_from_str(
                        "Cannot get upload date (startTimestamp)",
                    ))
                    .map(|s| s.to_owned());
            } else {
                return Err(ParsingError::parsing_error_from_str(
                    "Cannot get upload date (everyhting)",
                ));
            }
        }
    }

    pub fn description(&self, html: bool) -> Result<(String, bool), ParsingError> {
        if let Some(desc) = self.secondary_info_renderer.get("description") {
            let desc = text_from_object(desc, html)?;
            if let Some(desc) = desc {
                if !desc.is_empty() {
                    return Ok((desc, true));
                }
            }
        }
        if let Some(desc) = self
            .player_response
            .get("videoDetails")
            .and_then(|f| f.get("shortDescription").and_then(|f| f.as_str()))
        {
            return Ok((desc.to_string(), false));
        }
        Err(ParsingError::parsing_error_from_str("Cant get description"))
    }

    pub fn video_id(&self) -> String {
        self.video_id.clone()
    }

    pub fn video_thumbnails(&self) -> Result<Vec<Thumbnail>, ParsingError> {
        if let Value::Object(video_details) = self
            .player_response
            .get("videoDetails")
            .ok_or("cant get video Details")?
        {
            if let Value::Object(thumbnail) =
                video_details.get("thumbnail").ok_or("cant get thumbnail")?
            {
                if let Value::Array(thumbnails) = thumbnail
                    .get("thumbnails")
                    .ok_or("Cant get thumbnails array")?
                {
                    let mut thumbnails_str = vec![];
                    for thumb in thumbnails {
                        let mut thumbnail: Thumbnail =
                            serde_json::from_value(thumb.clone()).map_err(|e| e.to_string())?;
                        thumbnail.url = fix_thumbnail_url(&thumbnail.url);
                        thumbnails_str.push(thumbnail)
                    }
                    return Ok(thumbnails_str);
                }
            }
        }
        Err(ParsingError::parsing_error_from_str(
            "Cant get video thumbnails",
        ))
    }

    pub fn length(&self) -> Result<u64, ParsingError> {
        if let Some(duration) = self
            .player_response
            .get("videoDetails")
            .and_then(|f| f.get("lengthSeconds"))
            .and_then(|f| f.as_str())
        {
            if let Ok(duration) = duration.parse::<u64>() {
                return Ok(duration);
            }
        }
        if let Some(duration_ms) = self
            .player_response
            .get("streamingData")
            .and_then(|f| f.get("formats"))
            .and_then(|f| f.as_array())
            .and_then(|f| f.get(0))
            .and_then(|f| f.get("approxDurationMs"))
            .and_then(|f| f.as_str())
        {
            if let Ok(duration) = duration_ms.parse::<u64>() {
                return Ok(duration / 1000);
            }
        }

        Err(ParsingError::parsing_error_from_str("Cant get length"))
    }

    pub fn view_count(&self) -> Result<u128, ParsingError> {
        let mut views = String::new();
        if let Some(vc) = self
            .primary_info_renderer
            .get("viewCount")
            .and_then(|f| f.get("videoViewCountRenderer"))
            .and_then(|f| f.get("viewCount"))
        {
            views = text_from_object(vc, false)?.unwrap_or("".to_string());
        }
        if views.is_empty() {
            if let Some(vc) = self
                .player_response
                .get("videoDetails")
                .and_then(|f| f.get("viewCount"))
                .and_then(|f| f.as_str())
            {
                views = vc.to_string();
            }
        }
        if !views.is_empty() {
            if views.to_ascii_lowercase().contains("no views") {
                return Ok(0);
            } else {
                if let Ok(views) = remove_non_digit_chars::<u128>(&views) {
                    return Ok(views);
                }
            }
        }
        Err(ParsingError::parsing_error_from_str("Cant get view count"))
    }

    pub fn like_count(&self) -> Result<i128, ParsingError> {
        let mut like_string = String::new();
        if let Some(likes) = self
            .primary_info_renderer
            .get("sentimentBar")
            .and_then(|f| f.get("sentimentBarRenderer"))
            .and_then(|f| f.get("tooltip"))
            .and_then(|f| f.as_str())
        {
            if let Some(lks) = likes.split("/").next() {
                like_string = lks.to_string();
            }
        }
        if like_string.is_empty() {
            if let Some(allow_ratings) = self
                .player_response
                .get("videoDetails")
                .and_then(|f| f.get("allowRatings"))
                .and_then(|f| f.as_bool())
            {
                if allow_ratings {
                    return Err(ParsingError::parsing_error_from_str(
                        "Ratings are enabled even though the like button is missing",
                    ));
                } else {
                    return Ok(-1);
                }
            }
        } else {
            if let Ok(likes) = remove_non_digit_chars::<i128>(&like_string) {
                return Ok(likes);
            }
        }
        Err(ParsingError::parsing_error_from_str(
            "could not get like count",
        ))
    }

    pub fn dislike_count(&self) -> Result<i128, ParsingError> {
        let mut like_string = String::new();
        if let Some(likes) = self
            .primary_info_renderer
            .get("sentimentBar")
            .and_then(|f| f.get("sentimentBarRenderer"))
            .and_then(|f| f.get("tooltip"))
            .and_then(|f| f.as_str())
        {
            if let Some(lks) = likes.split("/").nth(1) {
                like_string = lks.to_string();
            }
        }
        if like_string.is_empty() {
            if let Some(allow_ratings) = self
                .player_response
                .get("videoDetails")
                .and_then(|f| f.get("allowRatings"))
                .and_then(|f| f.as_bool())
            {
                if allow_ratings {
                    return Err(ParsingError::parsing_error_from_str(
                        "Ratings are enabled even though the dislike button is missing",
                    ));
                } else {
                    return Ok(-1);
                }
            }
        } else {
            if let Ok(likes) = remove_non_digit_chars::<i128>(&like_string) {
                return Ok(likes);
            }
        }
        Err(ParsingError::parsing_error_from_str(
            "could not get dislike count",
        ))
    }

    pub fn uploader_url(&self) -> Result<String, ParsingError> {
        if let Some(nav_end) = self
            .secondary_info_renderer
            .get("owner")
            .and_then(|f| f.get("videoOwnerRenderer"))
            .and_then(|f| f.get("navigationEndpoint"))
        {
            let uploader_url = url_from_navigation_endpoint(nav_end)?;
            if !uploader_url.is_empty() {
                return Ok(uploader_url);
            }
        }
        if let Some(uploader_id) = self
            .player_response
            .get("videoDetails")
            .and_then(|f| f.get("channelId"))
            .and_then(|f| f.as_str())
        {
            return Ok(format!("https://www.youtube.com/channel/{}", uploader_id));
        }
        Err(ParsingError::parsing_error_from_str(
            "Cant get uploader url",
        ))
    }

    pub fn uploader_name(&self) -> Result<String, ParsingError> {
        let mut uploader_name = String::new();
        if let Some(uploader) = self
            .secondary_info_renderer
            .get("owner")
            .and_then(|f| f.get("videoOwnerRenderer"))
            .and_then(|f| f.get("title"))
        {
            if let Some(uploader) = text_from_object(uploader, false)? {
                uploader_name = uploader;
            }
        }
        if uploader_name.is_empty() {
            if let Some(author) = self
                .player_response
                .get("videoDetails")
                .and_then(|f| f.get("author"))
                .and_then(|f| f.as_str())
            {
                uploader_name = author.to_owned();
            }
        }

        if uploader_name.is_empty() {
            Err(ParsingError::parsing_error_from_str(
                "Cant get uploader name",
            ))
        } else {
            Ok(uploader_name)
        }
    }

    pub fn uploader_avatar_url(&self) -> Result<Vec<Thumbnail>, ParsingError> {
        let mut thumbnails = vec![];
        if let Some(thumbs) = self
            .secondary_info_renderer
            .get("owner")
            .and_then(|f| f.get("videoOwnerRenderer"))
            .and_then(|f| f.get("thumbnail"))
            .and_then(|f| f.get("thumbnails"))
            .and_then(|f| f.as_array())
        {
            for thumb in thumbs {
                if let Ok(mut thumb) = serde_json::from_value::<Thumbnail>(thumb.clone()) {
                    thumb.url = fix_thumbnail_url(&thumb.url);
                    thumbnails.push(thumb);
                }
            }
        }
        Ok(thumbnails)
    }
}

impl<D: Downloader> YTStreamExtractor<D> {
    pub fn video_stream(&self) -> Result<Vec<StreamItem>, ParsingError> {
        let mut video_streams = vec![];
        for entry in YTStreamExtractor::<D>::itags(
            &self.downloader,
            FORMATS,
            ItagType::Video,
            &self.player_response,
            &self.player_code,
        )? {
            let itag = entry.1;
            video_streams.push(StreamItem {
                url: Some(entry.0),
                ..itag
            });
        }
        Ok(video_streams)
    }

    pub fn video_only_stream(&self) -> Result<Vec<StreamItem>, ParsingError> {
        let mut video_streams = vec![];
        for entry in YTStreamExtractor::<D>::itags(
            &self.downloader,
            ADAPTIVE_FORMATS,
            ItagType::VideoOnly,
            &self.player_response,
            &self.player_code,
        )? {
            let itag = entry.1;
            video_streams.push(StreamItem {
                url: Some(entry.0),
                ..itag
            });
        }
        Ok(video_streams)
    }

    pub fn audio_streams(&self) -> Result<Vec<StreamItem>, ParsingError> {
        let mut audio_streams = vec![];
        for entry in YTStreamExtractor::<D>::itags(
            &self.downloader,
            ADAPTIVE_FORMATS,
            ItagType::Audio,
            &self.player_response,
            &self.player_code,
        )? {
            let itag = entry.1;
            audio_streams.push(StreamItem {
                url: Some(entry.0),
                ..itag
            });
        }

        Ok(audio_streams)
    }

    pub fn related(&self) -> Result<Vec<YTSearchItem>, ParsingError> {
        let results = (|| {
            self.initial_data
                .get("contents")?
                .get("twoColumnWatchNextResults")?
                .get("secondaryResults")?
                .get("secondaryResults")?
                .get("results")?
                .as_array()
                .cloned()
        })()
        .unwrap_or_default();
        use crate::youtube_extractor::search_extractor::YTSearchExtractor;
        let items = YTSearchExtractor::<D>::collect_streams_from(&results);
        items
    }
}
