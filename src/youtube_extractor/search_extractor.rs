use super::super::downloader_trait::Downloader;
use crate::youtube_extractor::channel_info_item_extractor::YTChannelInfoItemExtractor;
use crate::youtube_extractor::error::ParsingError;
use crate::youtube_extractor::playlist_info_item_extractor::YTPlaylistInfoItemExtractor;
use crate::youtube_extractor::stream_extractor::HARDCODED_CLIENT_VERSION;
use crate::youtube_extractor::stream_info_item_extractor::YTStreamInfoItemExtractor;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// https://url.spec.whatwg.org/#fragment-percent-encode-set
const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

#[derive(Clone, PartialEq)]
pub enum YTSearchItem {
    StreamInfoItem(YTStreamInfoItemExtractor),
    ChannelInfoItem(YTChannelInfoItemExtractor),
    PlaylistInfoItem(YTPlaylistInfoItemExtractor),
}

#[derive(Clone, PartialEq)]
pub struct YTSearchExtractor<D> {
    downloader: D,
    initial_data: Map<String, Value>,
    query: String,
    page: Option<(Vec<YTSearchItem>, Option<String>)>,
    p_url: Option<String>,
}

impl<D: Downloader> YTSearchExtractor<D> {
    async fn initial_data(
        downloader: &D,
        url: &str,
        page_count: &str,
    ) -> Result<Map<String, Value>, ParsingError> {
        let url = format!("{}&gl=US&pbj=1&page={}", url, page_count);
        let mut headers = HashMap::new();
        headers.insert("X-YouTube-Client-Name".to_string(), "1".to_string());
        headers.insert(
            "X-YouTube-Client-Version".to_string(),
            HARDCODED_CLIENT_VERSION.to_string(),
        );
        let resp = downloader.download_with_header(&url, headers).await?;
        let resp_json = serde_json::from_str::<Value>(&resp)
            .map_err(|er| ParsingError::parsing_error_from_str(&er.to_string()))?;
        let resp_json = resp_json
            .get(1)
            .ok_or("index 1 not in pbj")?
            .get("response")
            .ok_or("response not in pbj")?
            .as_object()
            .ok_or(format!("initial data not json object "))?
            .to_owned();
        Ok(resp_json)
    }

    pub fn collect_streams_from(videos: &Vec<Value>) -> Result<Vec<YTSearchItem>, ParsingError> {
        let mut search_items = vec![];
        for item in videos {
            if item.get("backgroundPromoRenderer").is_some() {
                return Err(ParsingError::from("Nothing found"));
            }
            if let Some(el) = item
                .get("videoRenderer")
                .or(item.get("compactVideoRenderer"))
                .map(|f| f.as_object())
            {
                if let Some(vid_info) = el {
                    search_items.push(YTSearchItem::StreamInfoItem(YTStreamInfoItemExtractor {
                        video_info: vid_info.to_owned(),
                    }));
                }
            } else if let Some(el) = item.get("channelRenderer").map(|f| f.as_object()) {
                if let Some(vid_info) = el {
                    search_items.push(YTSearchItem::ChannelInfoItem(YTChannelInfoItemExtractor {
                        channel_info: vid_info.to_owned(),
                    }));
                }
            } else if let Some(el) = item.get("playlistRenderer").map(|f| f.as_object()) {
                if let Some(vid_info) = el {
                    search_items.push(YTSearchItem::PlaylistInfoItem(
                        YTPlaylistInfoItemExtractor {
                            playlist_info: vid_info.to_owned(),
                        },
                    ));
                }
            }
        }

        Ok(search_items)
    }
}

impl<D: Downloader> YTSearchExtractor<D> {
    pub async fn new(
        downloader: D,
        query: &str,
        page_url: Option<String>,
    ) -> Result<YTSearchExtractor<D>, ParsingError> {
        let url = format!(
            "https://www.youtube.com/results?disable_polymer=1&search_query={}",
            query
        );
        let query = utf8_percent_encode(query, FRAGMENT).to_string();
        if let Some(page_url) = page_url {
            let initial_data =
                YTSearchExtractor::<D>::initial_data(&downloader, &url, &page_url).await?;

            Ok(YTSearchExtractor {
                downloader,
                initial_data,
                query,
                page: None,
                p_url: Some(page_url),
            })
        } else {
            let initial_data = YTSearchExtractor::<D>::initial_data(&downloader, &url, "1").await?;
            Ok(YTSearchExtractor {
                downloader,
                initial_data,
                query,
                page: None,
                p_url: Some("1".to_string()),
            })
        }
    }

    pub async fn search_suggestion(
        downloader: &D,
        query: &str,
    ) -> Result<Vec<String>, ParsingError> {
        let mut suggestions = vec![];
        let url = format!(
            "https://suggestqueries.google.com/complete/search\
            ?client=youtube\
            &jsonp=jp\
            &ds=yt\
            &q={}",
            query
        );
        let resp = downloader.download(&url).await?;
        let resp = resp[3..resp.len() - 1].to_string();
        let json =
            serde_json::from_str::<Value>(&resp).map_err(|e| ParsingError::from(e.to_string()))?;
        if let Some(collection) = (|| json.get(1)?.as_array())() {
            for suggestion in collection {
                if let Some(suggestion_str) = (|| suggestion.get(0)?.as_str())() {
                    suggestions.push(suggestion_str.to_string())
                }
            }
        }

        Ok(suggestions)
    }

    pub fn search_results(&self) -> Result<Vec<YTSearchItem>, ParsingError> {
        if let Some((items, _)) = &self.page {
            return Ok(items.clone());
        }
        let sections = (|| {
            let data = self
                .initial_data
                .get("contents")?
                .get("twoColumnSearchResultsRenderer")?
                .get("primaryContents")?
                .get("sectionListRenderer")?
                .get("contents")?
                .as_array()?;
            Some(data)
        })()
        .ok_or("cant get sections ")?;

        let mut search_items: Vec<YTSearchItem> = vec![];

        for sect in sections {
            let item_section = (|| {
                let c = sect
                    .get("itemSectionRenderer")?
                    .get("contents")?
                    .as_array()?;
                Some(c)
            })()
            .ok_or("cant get section");
            if let Ok(item_section) = item_section {
                search_items.append(&mut YTSearchExtractor::<D>::collect_streams_from(
                    &item_section,
                )?)
            }
        }
        return Ok(search_items);
    }

    pub fn next_page_url(&self) -> Result<Option<String>, ParsingError> {
        let pu = self
            .p_url
            .clone()
            .unwrap_or_default()
            .parse::<u32>()
            .map_err(|e| ParsingError::from(e.to_string()))?;
        return Ok(Some(format!("{}", pu + 1)));
    }
}
