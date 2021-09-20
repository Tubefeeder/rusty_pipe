extern crate rusty_pipe;

use rusty_pipe::extractors::{YTStreamInfoItemExtractor, YTTrendingExtractor};
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
    let extractor = YTTrendingExtractor::new(downloader).await?;

    let videos = extractor.videos()?;

    print_videos(videos);
    Ok(())
}
