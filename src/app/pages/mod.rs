#![doc = include_str!("./README.md")]
//! Page-level components.

mod dashboard;
mod experiment_detail;
mod experiments;
mod not_found;
mod settings;

pub(crate) use dashboard::Dashboard;
pub(crate) use experiment_detail::ExperimentDetail;
pub(crate) use experiments::Experiments;
pub(crate) use not_found::NotFound;
pub(crate) use settings::SettingsPage;
