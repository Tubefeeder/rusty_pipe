use crate::downloader_trait::Downloader;
use crate::utils::utils::{remove_non_digit_chars, text_from_object, url_from_navigation_endpoint};
use crate::youtube_extractor::error::ParsingError;
use crate::youtube_extractor::stream_extractor::{Thumbnail, HARDCODED_CLIENT_VERSION};
use crate::youtube_extractor::stream_info_item_extractor::YTStreamInfoItemExtractor;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, PartialEq)]
pub struct YTPlaylistExtractor<D> {
    downloader: D,
    init_data: Value,
    playlist_info: Value,
    page: Option<(Vec<YTStreamInfoItemExtractor>, Option<String>)>,
}

impl<D: Downloader> YTPlaylistExtractor<D> {
    pub async fn new(
        downloader: D,
        playlist_id: &str,
        page_url: Option<String>,
    ) -> Result<Self, ParsingError> {
        if let Some(page_url) = page_url {
            let initial_data = YTPlaylistExtractor::initial_data(&downloader, playlist_id);
            let page = YTPlaylistExtractor::page(&downloader, &page_url);
            use futures::try_join;
            let (initial_data, page) = try_join!(initial_data, page)?;
            let playlist_info = YTPlaylistExtractor::<D>::playlist_info(&initial_data)?;

            Ok(Self {
                downloader,
                init_data: initial_data,
                playlist_info,
                page: Some(page),
            })
        } else {
            let initial_data = YTPlaylistExtractor::initial_data(&downloader, playlist_id).await?;
            let playlist_info = YTPlaylistExtractor::<D>::playlist_info(&initial_data)?;
            Ok(Self {
                downloader,
                init_data: initial_data,
                playlist_info,
                page: None,
            })
        }
    }

    async fn initial_data(downloader: &D, id: &str) -> Result<Value, ParsingError> {
        let url = format!("https://www.youtube.com/playlist?list={}&pbj=1", id);
        let mut headers = HashMap::new();
        headers.insert("X-YouTube-Client-Name".to_string(), "1".to_string());
        headers.insert(
            "X-YouTube-Client-Version".to_string(),
            HARDCODED_CLIENT_VERSION.to_string(),
        );
        let response = downloader.download_with_header(&url, headers).await?;
        let json_response = serde_json::from_str::<Value>(&response)
            .map_err(|e| ParsingError::from(e.to_string()))?;
        let json_response = json_response
            .get(1)
            .ok_or("1 not in json resp")?
            .get("response")
            .ok_or("response not found")?;
        Ok(json_response.clone())
    }

    fn playlist_info(initial_data: &Value) -> Result<Value, ParsingError> {
        let pinfo = (|| {
            initial_data
                .get("sidebar")?
                .get("playlistSidebarRenderer")?
                .get("items")?
                .get(0)?
                .get("playlistSidebarPrimaryInfoRenderer")
        })();
        if let Some(pinfo) = pinfo {
            Ok(pinfo.clone())
        } else {
            Err(ParsingError::from("Cant get playlist info"))
        }
    }

    fn collect_streams_from(
        videos: &Value,
    ) -> Result<Vec<YTStreamInfoItemExtractor>, ParsingError> {
        let mut streams = vec![];
        let videos = videos.as_array().ok_or("Videos not array")?;
        for vid in videos {
            if let Some(video) = vid.get("playlistVideoRenderer") {
                if let Some(video) = video.as_object() {
                    streams.push(YTStreamInfoItemExtractor {
                        video_info: video.clone(),
                    })
                }
            }
        }
        Ok(streams)
    }

    fn next_page_url_from(continuation: &Value) -> Option<String> {
        let next_continuation_data = continuation.get(0)?.get("nextContinuationData")?;
        let continuation = next_continuation_data.get("continuation")?.as_str()?;
        let click_tracking_params = next_continuation_data
            .get("clickTrackingParams")?
            .as_str()?;
        Some(format!(
            "https://www.youtube.com/browse_ajax?ctoken={}&continuation={}&itct={}",
            continuation, continuation, click_tracking_params
        ))
    }

    async fn page(
        downloader: &D,
        page_url: &str,
    ) -> Result<(Vec<YTStreamInfoItemExtractor>, Option<String>), ParsingError> {
        let mut headers = HashMap::new();
        headers.insert("X-YouTube-Client-Name".to_string(), "1".to_string());
        headers.insert(
            "X-YouTube-Client-Version".to_string(),
            HARDCODED_CLIENT_VERSION.to_string(),
        );
        let response = downloader.download_with_header(&page_url, headers).await?;
        let json_response = serde_json::from_str::<Value>(&response)
            .map_err(|e| ParsingError::from(e.to_string()))?;

        let section_list_continuation = (|| {
            json_response
                .get(1)?
                .get("response")?
                .get("continuationContents")?
                .get("playlistVideoListContinuation")
        })()
        .ok_or("Cant get continuation")?;

        let items = YTPlaylistExtractor::<D>::collect_streams_from(
            section_list_continuation
                .get("contents")
                .ok_or("items not in continuation")?,
        )?;
        let next_url = YTPlaylistExtractor::<D>::next_page_url_from(
            section_list_continuation
                .get("continuations")
                .unwrap_or(&Value::Null),
        );

        Ok((items, next_url))
    }
}

impl<D: Downloader> YTPlaylistExtractor<D> {
    pub fn name(&self) -> Result<String, ParsingError> {
        if let Some(title) = self.playlist_info.get("title") {
            let name = text_from_object(title, false)?;
            if let Some(name) = name {
                if !name.is_empty() {
                    return Ok(name);
                }
            }
        }
        let title = (|| {
            self.init_data
                .get("microformat")?
                .get("microformatDataRenderer")?
                .get("title")?
                .as_str()
        })();
        if let Some(title) = title {
            return Ok(title.to_string());
        }
        Err(ParsingError::from("Cant get name"))
    }

    pub fn thumbnails(&self) -> Result<Vec<Thumbnail>, ParsingError> {
        let mut thumbnails = vec![];
        for thumb in (|| {
            self.playlist_info
                .get("thumbnailRenderer")?
                .get("playlistVideoThumbnailRenderer")?
                .get("thumbnail")?
                .get("thumbnails")?
                .as_array()
        })()
        .or((|| {
            self.init_data
                .get("microformat")?
                .get("microformatDataRenderer")?
                .get("thumbnail")?
                .get("thumbnails")?
                .as_array()
        })())
        .ok_or("Cant get thumbnails")?
        {
            if let Ok(thumb) = serde_json::from_value(thumb.to_owned()) {
                thumbnails.push(thumb)
            }
        }
        Ok(thumbnails)
    }

    fn uploader_info(&self) -> Result<Value, ParsingError> {
        let items = (|| {
            self.init_data
                .get("sidebar")?
                .get("playlistSidebarRenderer")?
                .get("items")?
                .as_array()
        })();
        if let Some(items) = items {
            for item in items {
                if let Some(video_owner) = (|| {
                    item.get("playlistSidebarSecondaryInfoRenderer")?
                        .get("videoOwner")?
                        .get("videoOwnerRenderer")
                })() {
                    return Ok(video_owner.clone());
                }
            }
        }
        Err(ParsingError::from("Cant get uploader info"))
    }

    pub fn uploader_url(&self) -> Result<String, ParsingError> {
        if let Some(navp) = self.uploader_info()?.get("navigationEndpoint") {
            return Ok(url_from_navigation_endpoint(navp)?);
        } else {
            Err(ParsingError::from("Cant get uploader url"))
        }
    }
    pub fn uploader_name(&self) -> Result<String, ParsingError> {
        if let Some(navp) = self.uploader_info()?.get("title") {
            return Ok(text_from_object(navp, false)?.ok_or("uploader name not found")?);
        } else {
            Err(ParsingError::from("Cant get uploader url"))
        }
    }

    pub fn uploader_avatars(&self) -> Result<Vec<Thumbnail>, ParsingError> {
        let mut thumbnails = vec![];
        let uploader = self.uploader_info()?;
        for thumb in (|| uploader.get("thumbnail")?.get("thumbnails")?.as_array())()
            .ok_or("Cant get uploaader thumbnails")?
        {
            if let Ok(thumb) = serde_json::from_value(thumb.to_owned()) {
                thumbnails.push(thumb)
            }
        }
        Ok(thumbnails)
    }

    pub fn stream_count(&self) -> Result<i32, ParsingError> {
        let views_text = text_from_object(
            self.playlist_info
                .get("stats")
                .ok_or("No stats")?
                .get(0)
                .ok_or("No 0 in stats")?,
            false,
        )?
        .unwrap_or_default();
        let videoc = remove_non_digit_chars::<i32>(&views_text)
            .map_err(|e| ParsingError::from(e.to_string()))?;
        Ok(videoc)
    }

    pub fn videos(&self) -> Result<Vec<YTStreamInfoItemExtractor>, ParsingError> {
        if let Some((videos, _)) = &self.page {
            return Ok(videos.clone());
        }
        let videos = (|| {
            self.init_data
                .get("contents")?
                .get("twoColumnBrowseResultsRenderer")?
                .get("tabs")?
                .get(0)?
                .get("tabRenderer")?
                .get("content")?
                .get("sectionListRenderer")?
                .get("contents")?
                .get(0)?
                .get("itemSectionRenderer")?
                .get("contents")?
                .get(0)?
                .get("playlistVideoListRenderer")?
                .get("contents")
        })()
        .ok_or("Cant get videos")?;
        YTPlaylistExtractor::<D>::collect_streams_from(videos)
    }

    pub fn next_page_url(&self) -> Result<Option<String>, ParsingError> {
        if let Some((_, page_url)) = &self.page {
            Ok(page_url.clone())
        } else {
            let conti = (|| {
                self.init_data
                    .get("contents")?
                    .get("twoColumnBrowseResultsRenderer")?
                    .get("tabs")?
                    .get(0)?
                    .get("tabRenderer")?
                    .get("content")?
                    .get("sectionListRenderer")?
                    .get("contents")?
                    .get(0)?
                    .get("itemSectionRenderer")?
                    .get("contents")?
                    .get(0)?
                    .get("playlistVideoListRenderer")?
                    .get("continuations")
            })();
            if let Some(conti) = conti {
                Ok(YTPlaylistExtractor::<D>::next_page_url_from(conti))
            } else {
                println!("Continuation is None");
                Ok(None)
            }
        }
    }
}
