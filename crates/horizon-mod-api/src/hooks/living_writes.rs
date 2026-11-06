//! The `living_writes` hooks and helpers: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Helpers {
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_swing"
    )]
    fn living_swing();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_arrow_count"
    )]
    fn living_arrow_count();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_set_arrow_count"
    )]
    fn living_set_arrow_count();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_stinger_count"
    )]
    fn living_stinger_count();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_set_stinger_count"
    )]
    fn living_set_stinger_count();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_set_arrow_removal_ticks"
    )]
    fn living_set_arrow_removal_ticks();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_set_stinger_removal_ticks"
    )]
    fn living_set_stinger_removal_ticks();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_item_use_remaining"
    )]
    fn living_item_use_remaining();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_set_item_use_remaining"
    )]
    fn living_set_item_use_remaining();
    #[helper(
        side = "both",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_used_item_hand"
    )]
    fn living_used_item_hand();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_start_using_item"
    )]
    fn living_start_using_item();
    #[helper(
        side = "server",
        receiver = "living",
        target = "horizon_client::game::native_living_writes::Game<'_>::native_living_release_using_item"
    )]
    fn living_release_using_item();
}
