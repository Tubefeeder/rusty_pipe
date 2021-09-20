extern crate rusty_pipe;

use rusty_pipe::extractors::YTStreamExtractor;
use rusty_pipe::Downloader;
use rusty_pipe::ParsingError;

use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;

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
async fn main() -> Result<(), failure::Error> {
    pretty_env_logger::init();

    let downloader = DownloaderExample(reqwest::Client::new());

    let stream_extractor = YTStreamExtractor::new(downloader, "09R8_2nJtjg").await?;
    let video_streams = stream_extractor.video_stream()?;
    println!("AUDIO/VIDEO STREAMS \n");
    println!("{:#?}", video_streams);

    let audio_streams = stream_extractor.audio_streams()?;
    println!("AUDIO ONLY STREAMS \n");
    println!("{:#?}", audio_streams);

    let video_only_streams = stream_extractor.video_only_stream()?;
    println!("VIDEO ONLY STREAMS \n");
    println!("{:#?}", video_only_streams);

    let thumbnails = stream_extractor.video_thumbnails();
    println!("\nTHUMBNAILS");
    println!("{:#?}", thumbnails);

    println!("\nMETADATA");
    println!("title: {:#?}", stream_extractor.name());
    println!("description:\n{:#?}", stream_extractor.description(false));
    println!("uploaded:\n{:#?}", stream_extractor.upload_date());
    println!("duration: {:#?}", stream_extractor.length());
    println!("views: {:#?}", stream_extractor.view_count());
    println!("likes: {:#?}", stream_extractor.like_count());
    println!("dislikes: {:#?}", stream_extractor.dislike_count());
    println!("uploader url: {:#?}", stream_extractor.uploader_url());
    println!("uploader name: {:#?}", stream_extractor.uploader_name());
    println!(
        "uploader thumbnails:\n {:#?}",
        stream_extractor.uploader_avatar_url()
    );
    Ok(())
}
