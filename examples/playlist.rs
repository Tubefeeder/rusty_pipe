extern crate rusty_pipe;

use rusty_pipe::extractors::{YTPlaylistExtractor, YTStreamInfoItemExtractor};
use rusty_pipe::Downloader;
use rusty_pipe::ParsingError;

use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use failure::Error;

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

fn print_videos(videos: Vec<YTStreamInfoItemExtractor>) {
    let mut count = 0;
    for vid in videos {
        count += 1;
        println!("STREAM {}", count);
        println!("title: {:#?}", vid.name());
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let downloader = DownloaderExample(reqwest::Client::new());
    println!("Enter playlist id: ");
    let mut playlist_id = String::new();
    std::io::stdin()
        .read_line(&mut playlist_id)
        .expect("Input failed");
    playlist_id = playlist_id.trim().to_string();
    let playlist_extractor =
        YTPlaylistExtractor::new(downloader.clone(), &playlist_id, None).await?;
    println!("Playlist name {:#?}", playlist_extractor.name());
    println!(
        "Playlist Thumbnails \n{:#?}",
        playlist_extractor.thumbnails()
    );
    println!("Uploader name: {:#?}", playlist_extractor.uploader_name());
    println!("Uploader url: {:#?}", playlist_extractor.uploader_url());
    println!(
        "Uploaders thumbnails \n{:#?}",
        playlist_extractor.uploader_avatars()
    );

    println!("Videos count : {:#?}", playlist_extractor.stream_count());

    println!("Videos :\n");
    let mut videos = vec![];
    videos.append(&mut playlist_extractor.videos()?);
    println!("Next Page url: {:#?}", playlist_extractor.next_page_url());

    let mut next_page_url = playlist_extractor.next_page_url()?;

    while let Some(next_page) = next_page_url.clone() {
        let extractor =
            YTPlaylistExtractor::new(downloader.clone(), &playlist_id, Some(next_page)).await?;
        next_page_url = extractor.next_page_url()?;
        videos.append(&mut playlist_extractor.videos()?);
        println!("Next page url {:#?}", next_page_url);
    }
    print_videos(videos);

    Ok(())
}
