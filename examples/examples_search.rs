extern crate rusty_pipe;

use rusty_pipe::youtube_extractor::search_extractor::*;
use scraper::Html;
use std::io;

use urlencoding::encode;

fn main() {
    static APP_USER_AGENT: &str =
        "Mozilla/5.0 (X11; Linux x86_64; rv:68.0) Gecko/20100101 Firefox/43.0";

    let client: reqwest::blocking::Client = reqwest::blocking::Client::builder()
        .user_agent(APP_USER_AGENT)
        .build()
        .expect("Cannot build client");

    let mut search_query = String::new();
    println!("Enter Search Query");
    io::stdin()
        .read_line(&mut search_query)
        .expect("Cannot Read Input");

    search_query = encode(&search_query);

    let resp = client
        .get(&format!(
            "https://www.youtube.com/results?search_query={}",
            search_query
        ))
        .send()
        .expect("Cannot send");

    assert!(resp.status().is_success());

    let body = resp.text().unwrap();
    let doc = Html::parse_document(&body);

    let search_extractor = YTSearchExtractor { doc };
    let search_suggestion = search_extractor.get_search_suggestion();

    match search_suggestion {
        None => println!("No search Suggestion"),
        Some(str) => println!("Search Suggestion {}", str),
    }
    let items = search_extractor.collect_items();
    println!("Items Found {}", items.len());
    println!();

    for item in items {
        match item {
            YTSearchItem::StreamInfoItem(streaminfoitem) => {
                println!("Stream");
                println!(
                    "title : {}",
                    streaminfoitem.get_name().expect("Stream has no title")
                );
                println!(
                    "URL : {}",
                    streaminfoitem.get_url().expect("Stream has no url")
                );
                println!("isLive: {}", streaminfoitem.is_live());
                match streaminfoitem.get_duration() {
                    None => println!("Duration: Unknown"),
                    Some(d) => println!("Duration: {}s", d),
                }
                println!(
                    "Uploader: {}",
                    streaminfoitem.get_uploader_name().unwrap_or("Unknown")
                );
                println!(
                    "Uploader Url: {}",
                    streaminfoitem
                        .get_uploader_url()
                        .unwrap_or("Unknown".to_owned())
                );
                println!(
                    "Upload Date: {}",
                    streaminfoitem
                        .get_textual_upload_date()
                        .unwrap_or("Unknown")
                );
                println!(
                    "View Count: {}",
                    streaminfoitem
                        .get_view_count()
                        .map_or("Unknown".to_owned(), |c| c.to_string())
                );
                println!(
                    "Thumbnail Url: {}",
                    streaminfoitem
                        .get_thumbnail_url()
                        .unwrap_or("Unknown".to_owned())
                );

                println!();
            }
            YTSearchItem::ChannelInfoItem(channel_info_item) => {
                println!("Channel");
                println!(
                    "Name : {}",
                    channel_info_item.get_name().unwrap_or("Unknown")
                );
                println!(
                    "Url : {}",
                    channel_info_item.get_url().unwrap_or("Unknown".to_owned())
                );
                println!(
                    "Thumbnail Url : {}",
                    channel_info_item.get_thumbnail_url().unwrap_or("Unknown")
                );
                println!(
                    "Subscriber's count : {}",
                    channel_info_item
                        .get_subscriber_count()
                        .map_or("Unknown".to_owned(), |c| c.to_string())
                );
                println!(
                    "Description : {}",
                    channel_info_item.get_description().unwrap_or("Unknown")
                );
                println!(
                    "Stream Count : {}",
                    channel_info_item
                        .get_stream_count()
                        .map_or("Unknown".to_owned(), |c| c.to_string())
                );

                println!();
            }
            YTSearchItem::PlaylistInfoItem(playlist_info_item) => {
                println!("Playlist");
                println!(
                    "Name : {}",
                    playlist_info_item
                        .get_name()
                        .unwrap_or("Unknown".to_owned())
                );
                println!(
                    "Url : {}",
                    playlist_info_item.get_url().unwrap_or("Unknown".to_owned())
                );
                println!(
                    "Thumbnail Url : {}",
                    playlist_info_item.get_thumbnail_url().unwrap_or("Unknown")
                );
                println!(
                    "Uploader Name : {}",
                    playlist_info_item.get_uploader_name().unwrap_or("Unknown")
                );
                println!(
                    "Stream Count : {}",
                    playlist_info_item
                        .get_stream_count()
                        .map_or("Unknown".to_owned(), |d| d.to_string())
                );

                println!();
            }
        }
    }
}
