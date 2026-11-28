//! The distribution channel a binary is built for (ADR-0011), fixed at compile time by the `channel-mas` feature.
//!
//! A channel decides which capabilities the binary contains at all. A capability a channel lacks is compiled out, not
//! switched off: `build.rs` sets one `cfg` per capability from the same table as [`Channel::caps`], and the code behind
//! it is `#[cfg(...)]`-gated. Model families are a separate, narrower axis (`editions.rs`, ADR-0012).

use serde::Serialize;

#[cfg(all(feature = "channel-mas", feature = "lens-profiles"))]
compile_error!(
    "the Mac App Store build must not contain lens profiles: build it with `--no-default-features --features channel-mas`"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Direct,
    MacAppStore,
}

/// What a channel's binary contains. Every field is a whole feature that is present or compiled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChannelCaps {
    /// Running a user-configured program after export (`export_plugin.rs`). Apple guideline 2.5.2.
    pub export_command_hook: bool,
    /// Fetching model weights at run time. Apple guideline 2.4.5(iv).
    pub model_downloads: bool,
    /// Moving the thumbnail and preview cache to a folder the user picks (`storage::move_cache_dir`).
    pub custom_cache_dir: bool,
}

impl Channel {
    pub const fn caps(self) -> ChannelCaps {
        match self {
            Channel::Direct => ChannelCaps { export_command_hook: true, model_downloads: true, custom_cache_dir: true },
            Channel::MacAppStore => ChannelCaps { export_command_hook: false, model_downloads: false, custom_cache_dir: false },
        }
    }
}

pub const CURRENT: Channel = if cfg!(feature = "channel-mas") { Channel::MacAppStore } else { Channel::Direct };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_has_every_capability() {
        assert_eq!(
            Channel::Direct.caps(),
            ChannelCaps { export_command_hook: true, model_downloads: true, custom_cache_dir: true }
        );
    }

    #[test]
    fn the_mac_app_store_has_no_export_hook_no_downloads_and_no_cache_move() {
        assert_eq!(
            Channel::MacAppStore.caps(),
            ChannelCaps { export_command_hook: false, model_downloads: false, custom_cache_dir: false }
        );
    }

    #[test]
    fn the_compiled_cfgs_match_the_current_channels_row() {
        let caps = CURRENT.caps();
        assert_eq!(caps.export_command_hook, cfg!(export_command_hook));
        assert_eq!(caps.model_downloads, cfg!(model_downloads));
        assert_eq!(caps.custom_cache_dir, cfg!(custom_cache_dir));
    }

    #[test]
    fn the_channel_mas_feature_selects_the_mac_app_store() {
        assert_eq!(CURRENT == Channel::MacAppStore, cfg!(feature = "channel-mas"));
    }
}
