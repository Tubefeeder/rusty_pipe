extern crate rusty_pipe;

use rusty_pipe::elements::YTSearchItem;
use rusty_pipe::extractors::YTSearchExtractor;
use rusty_pipe::Downloader;
use rusty_pipe::ParsingError;

use std::collections::HashMap;
use std::io;
use std::str::FromStr;

use async_trait::async_trait;
use failure::Error;
use urlencoding::encode;

#[derive(Clone)]
struct DownloaderExample(reqwest::Client);

#[async_trait]
impl Downloader for DownloaderExample {
    async fn download(&self, url: &str) -> Result<String, ParsingError> {
        println!("query url : {}", url);
        let resp = self
            .0
            .get(url)
            .send()
            .await
            .map_err(|er| ParsingError::DownloadError {
                cause: er.to_string(),
            })?;
        println!("got response ");
        let body = resp
            .text()
            .await
            .map_err(|er| ParsingError::DownloadError {
                cause: er.to_string(),
            })?;
        println!("suceess query");
        Ok(String::from(body))
    }

    async fn download_with_header(
        &self,
        url: &str,
        header: HashMap<String, String>,
    ) -> Result<String, ParsingError> {
        let res = self.0.get(url);
        let mut headers = reqwest::header::HeaderMap::new();
        for header in header {
            headers.insert(
                reqwest::header::HeaderName::from_str(&header.0).map_err(|e| e.to_string())?,
                header.1.parse().unwrap(),
            );
        }
        let res = res.headers(headers);
        let res = res.send().await.map_err(|er| er.to_string())?;
        let body = res.text().await.map_err(|er| er.to_string())?;
        Ok(String::from(body))
    }

    fn eval_js(&self, script: &str) -> Result<String, String> {
        use quick_js::Context;
        let context = Context::new().expect("Cant create js context");
        println!("jscode \n{}", script);
        let res = context.eval(script).unwrap_or(quick_js::JsValue::Null);
        let result = res.into_string().unwrap_or("".to_string());
        print!("JS result: {}", result);
        Ok(result)
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let downloader = DownloaderExample(reqwest::Client::new());
    let mut search_query = String::new();
    println!("Enter Search Query");
    io::stdin()
        .read_line(&mut search_query)
        .expect("Cannot Read Input");

    search_query = encode(&search_query);

    let search_extractor = YTSearchExtractor::new(downloader.clone(), &search_query, None).await?;
    let search_suggestion =
        YTSearchExtractor::search_suggestion(&downloader, &search_query).await?;

    println!("Search suggestion {:#?}", search_suggestion);
    let mut items = search_extractor.search_results()?;
    let mut next_url = search_extractor.next_page_url()?;
    println!("Next page url : {:#?}", next_url);
    let mut max_page = 5;
    while let Some(url) = next_url.clone() {
        max_page -= 1;
        if max_page < 0 {
            break;
        }
        let search_extractor =
            YTSearchExtractor::new(downloader.clone(), &search_query, Some(url)).await?;
        items.append(&mut search_extractor.search_results()?);
        next_url = search_extractor.next_page_url()?;
        println!("Next page url : {:#?}", next_url);
    }
    println!("Items Found {}", items.len());
    println!();

    for item in items {
        match item {
            YTSearchItem::StreamInfoItem(streaminfoitem) => {
                println!("Stream");
                println!(
                    "title : {}",
                    streaminfoitem.name().expect("Stream has no title")
                );
                println!("id: {:#?}", streaminfoitem.video_id());
                println!("URL : {}", streaminfoitem.url().expect("Stream has no url"));
                println!("isLive: {:#?}", streaminfoitem.is_live());
                println!("Duration: {:#?}", streaminfoitem.duration());
                println!(
                    "Uploader: {:#?}",
                    streaminfoitem
                        .uploader_name()
                        .unwrap_or("Unknown".to_string())
                );
                println!(
                    "Uploader Url: {}",
                    streaminfoitem
                        .uploader_url()
                        .unwrap_or("Unknown".to_owned())
                );
                println!("Upload Date: {:#?}", streaminfoitem.textual_upload_date());
                println!("View Count: {:#?}", streaminfoitem.view_count());
                println!("Thumbnails:\n {:#?}", streaminfoitem.thumbnails());
                println!(
                    "Uploader Thumbnails:\n {:#?}",
                    streaminfoitem.uploader_thumbnails()
                );

                println!();
            }
            YTSearchItem::ChannelInfoItem(channel_info_item) => {
                println!("Channel");
                println!(
                    "Name : {}",
                    channel_info_item.name().unwrap_or("Unknown".to_string())
                );
                println!("Channel Id : {:#?}", channel_info_item.channel_id());
                println!(
                    "Url : {}",
                    channel_info_item.url().unwrap_or("Unknown".to_owned())
                );
                println!("Thumbnails \n{:#?}", channel_info_item.thumbnails());
                println!(
                    "Subscriber's count : {:#?}",
                    channel_info_item.subscriber_count()
                );
                println!("Description : {:#?}", channel_info_item.description());
                println!(
                    "Stream Count : {}",
                    channel_info_item
                        .stream_count()
                        .map_or("Unknown".to_owned(), |c| c.to_string())
                );

                println!();
            }
            YTSearchItem::PlaylistInfoItem(playlist_info_item) => {
                println!("Playlist");
                println!(
                    "Name : {}",
                    playlist_info_item.name().unwrap_or("Unknown".to_owned())
                );
                println!(
                    "Url : {}",
                    playlist_info_item.url().unwrap_or("Unknown".to_owned())
                );
                println!("Thumbnails \n{:#?}", playlist_info_item.thumbnails());
                println!(
                    "Uploader Name : {}",
                    playlist_info_item
                        .uploader_name()
                        .unwrap_or("Unknown".to_string())
                );
                println!("Stream Count : {:#?}", playlist_info_item.stream_count());

                println!();
            }
        }
    }

    Ok(())
}
