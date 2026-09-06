//! The screens. Every one of them draws from `AppState` and asks `AppState` to change things;
//! none of them touches a file. See the note on neighbourhood 1 in [`crate::app`].

pub mod controls;
pub mod date_blocks;
pub mod edit_form;
pub mod list_view;
pub mod onboarding_view;
pub mod search_view;
pub mod settings_view;
pub mod tips_view;
