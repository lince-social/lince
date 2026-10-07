#![recursion_limit = "256"]

#[cfg(feature = "native-media")]
pub mod communication;

#[cfg(feature = "native-runtime")]
pub mod app;

#[cfg(feature = "native-runtime")]
pub mod drawing;

#[cfg(feature = "native-runtime")]
pub mod owner_backup;

#[cfg(feature = "native-runtime")]
pub mod actions;

#[cfg(feature = "native-runtime")]
mod accessibility;

#[cfg(feature = "native-runtime")]
pub mod notifications;

#[cfg(feature = "native-runtime")]
pub mod information;

#[cfg(feature = "native-runtime")]
pub mod icons;

#[cfg(feature = "native-runtime")]
pub mod castle;

#[cfg(feature = "native-runtime")]
pub mod schedule_editor;
#[cfg(feature = "native-runtime")]
pub mod time_castle;

#[cfg(feature = "native-runtime")]
pub mod sound_cues;

#[cfg(feature = "native-runtime")]
mod castle_feed;

#[cfg(feature = "native-runtime")]
pub mod assertion_castle;

#[cfg(feature = "native-runtime")]
mod assertion_editor;

#[cfg(feature = "native-runtime")]
pub mod shader_castle;

#[cfg(feature = "native-runtime")]
pub mod karma_castle;

#[cfg(feature = "native-runtime")]
pub mod frequency_castle;

#[cfg(feature = "native-runtime")]
pub mod access_control;
pub mod workspace_sync;

#[cfg(feature = "native-runtime")]
pub mod sync_castle;

#[cfg(feature = "native-runtime")]
pub mod color_picker;

#[cfg(feature = "native-runtime")]
pub mod canvas;

#[cfg(feature = "native-runtime")]
pub mod canvas_host;

#[cfg(feature = "native-runtime")]
pub mod topology;

#[cfg(feature = "native-runtime")]
pub mod sand_placement;

#[cfg(feature = "native-runtime")]
pub mod canvas_item;

#[cfg(feature = "native-runtime")]
pub mod canvas_background;

#[cfg(feature = "native-runtime")]
mod canvas_colors;

#[cfg(feature = "native-runtime")]
mod canvas_clip;

#[cfg(feature = "native-runtime")]
mod canvas_pan;

#[cfg(feature = "native-runtime")]
mod canvas_resize;

#[cfg(feature = "native-runtime")]
pub mod canvas_controls;

#[cfg(feature = "native-runtime")]
pub mod edit_mode;

#[cfg(feature = "native-runtime")]
pub mod shortcuts;

#[cfg(feature = "native-runtime")]
pub mod deletion;

#[cfg(feature = "native-runtime")]
pub mod operation;

#[cfg(feature = "native-runtime")]
pub mod inspection;

#[cfg(feature = "native-runtime")]
pub mod workspace;

#[cfg(feature = "native-runtime")]
pub mod workspace_config;

#[cfg(feature = "native-runtime")]
pub mod physics;

#[cfg(feature = "native-runtime")]
pub mod area_effects;

#[cfg(feature = "native-runtime")]
pub(crate) mod influence_report;

#[cfg(feature = "native-runtime")]
pub mod sand_store;

#[cfg(feature = "native-runtime")]
pub mod custom_castle;

#[cfg(feature = "native-runtime")]
pub mod sand_text;

#[cfg(feature = "native-runtime")]
pub mod scroll_sand;

#[cfg(feature = "native-runtime")]
pub mod layout;

#[cfg(feature = "native-runtime")]
mod sand_text_editor;

#[cfg(feature = "native-runtime")]
pub mod container;

#[cfg(feature = "native-runtime")]
pub mod credits;

#[cfg(feature = "native-runtime")]
pub mod effect;

#[cfg(feature = "native-runtime")]
pub mod scoped_events;

#[cfg(feature = "native-runtime")]
pub mod calendar;
#[cfg(feature = "native-runtime")]
pub mod kanban;
#[cfg(feature = "native-runtime")]
pub mod simulation_castle;

#[cfg(feature = "native-runtime")]
pub mod time_limit;

#[cfg(feature = "native-runtime")]
pub mod sand;
#[cfg(feature = "native-runtime")]
mod save_feedback;

#[cfg(feature = "native-runtime")]
pub mod slider;

#[cfg(feature = "native-runtime")]
pub mod dropdown;

#[cfg(feature = "native-runtime")]
mod token_metrics;

#[cfg(feature = "native-runtime")]
pub mod theme;

#[cfg(feature = "native-runtime")]
pub use app::run_native_interface;

#[cfg(feature = "native-runtime")]
pub mod wake;

#[cfg(feature = "native-runtime")]
pub mod cell_bridge;

#[cfg(feature = "native-runtime")]
pub mod component_push;

#[cfg(feature = "native-runtime")]
pub mod record_view;

#[cfg(feature = "native-runtime")]
pub mod record_binding;
pub(crate) mod record_extensions;

#[cfg(feature = "native-runtime")]
pub mod work_timer;

#[cfg(feature = "native-runtime")]
pub mod arrow_sand;
#[cfg(feature = "native-runtime")]
pub mod full_record;
#[cfg(feature = "native-runtime")]
pub mod protein_motion;
#[cfg(feature = "native-runtime")]
pub mod record_presentation;
#[cfg(feature = "native-runtime")]
pub mod relation_castle;

#[cfg(feature = "native-runtime")]
pub mod record_creation;

#[cfg(feature = "native-runtime")]
pub mod thread_castle;

#[cfg(feature = "native-runtime")]
mod directory_list;
#[cfg(feature = "native-runtime")]
mod message_commands;
#[cfg(feature = "native-runtime")]
pub mod message_content;
#[cfg(feature = "native-runtime")]
mod message_progress;
#[cfg(feature = "native-runtime")]
mod message_questions;
#[cfg(feature = "native-runtime")]
mod operation_view;
#[cfg(feature = "native-runtime")]
mod question_form;
#[cfg(feature = "native-runtime")]
pub mod speech;

#[cfg(feature = "native-runtime")]
pub mod protein_area;
#[cfg(feature = "native-runtime")]
pub mod protein_castle;

#[cfg(feature = "native-runtime")]
pub mod instance;

#[cfg(feature = "native-runtime")]
pub mod instinct;

#[cfg(feature = "native-runtime")]
pub mod description;

#[cfg(feature = "native-runtime")]
pub mod fiote;

#[cfg(feature = "native-runtime")]
pub mod tutorial;
#[cfg(feature = "native-runtime")]
mod practice_cells;

#[cfg(feature = "native-runtime")]
pub mod tray;

#[cfg(feature = "native-runtime")]
pub mod tokens;

#[cfg(feature = "native-runtime")]
pub mod token_style;

#[cfg(feature = "native-runtime")]
pub mod customization;
#[cfg(feature = "native-runtime")]
pub mod sand_settings;

#[cfg(feature = "native-runtime")]
pub mod area;

#[cfg(feature = "native-runtime")]
pub mod area_mutation;

#[cfg(feature = "native-runtime")]
mod area_mutation_panel;

#[cfg(feature = "native-runtime")]
pub mod area_panel;

#[cfg(feature = "native-runtime")]
mod area_input;

#[cfg(feature = "native-runtime")]
mod area_target;

#[cfg(feature = "native-runtime")]
mod area_drawing;

#[cfg(feature = "native-runtime")]
pub mod canvas_selection;

#[cfg(feature = "native-runtime")]
pub mod laboratory;

#[cfg(feature = "native-runtime")]
mod sand_panel;

#[cfg(feature = "native-runtime")]
pub mod freedoom;

pub mod command_castle;
#[cfg(feature = "native-runtime")]
pub mod terminal;

#[cfg(feature = "native-runtime")]
pub mod configuration;

#[cfg(feature = "native-runtime")]
pub mod todo;

#[cfg(feature = "native-runtime")]
pub mod ontology;

#[cfg(feature = "native-runtime")]
pub mod sound;

#[cfg(feature = "native-runtime")]
pub mod sound_area;

#[cfg(feature = "native-runtime")]
pub mod recorder_castle;

#[cfg(feature = "native-runtime")]
pub mod document_viewer;
#[cfg(feature = "native-runtime")]
pub mod external_drop;
#[cfg(feature = "native-runtime")]
pub mod file_explorer;
#[cfg(feature = "native-runtime")]
pub mod ide;
#[cfg(feature = "native-runtime")]
pub mod media_sand;

#[cfg(feature = "native-runtime")]
pub mod transfer_castle;

#[cfg(feature = "native-runtime")]
pub mod organ_castle;
