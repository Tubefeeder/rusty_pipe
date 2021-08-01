mod downloader_trait;
mod utils;
mod youtube_extractor;

pub use crate::downloader_trait::Downloader;
pub use crate::youtube_extractor::error::ParsingError;
pub use crate::youtube_extractor::stream_extractor::HARDCODED_CLIENT_VERSION;

pub mod itag {
    pub use crate::youtube_extractor::itag_item::{Itag, ItagType};
}

pub mod elements {
    pub use crate::youtube_extractor::search_extractor::YTSearchItem;
    pub use crate::youtube_extractor::stream_extractor::{StreamItem, Thumbnail};
}

pub mod extractors {
    pub use crate::youtube_extractor::channel_extractor::YTChannelExtractor;
    pub use crate::youtube_extractor::channel_info_item_extractor::YTChannelInfoItemExtractor;
    pub use crate::youtube_extractor::playlist_extractor::YTPlaylistExtractor;
    pub use crate::youtube_extractor::playlist_info_item_extractor::YTPlaylistInfoItemExtractor;
    pub use crate::youtube_extractor::search_extractor::YTSearchExtractor;
    pub use crate::youtube_extractor::stream_extractor::YTStreamExtractor;
    pub use crate::youtube_extractor::stream_info_item_extractor::YTStreamInfoItemExtractor;
    pub use crate::youtube_extractor::trending_extractor::YTTrendingExtractor;
}
