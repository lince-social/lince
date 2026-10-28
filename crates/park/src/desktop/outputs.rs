use super::*;
use crate::commands::OutputPreference;
use lince_interface::host::display::{DisplayConfiguration, OutputMode};
use std::collections::BTreeMap;

impl Desktop {
    pub(super) fn restore_virtual_preferences(&mut self, preferences: &BTreeMap<String, OutputPreference>) -> io::Result<()> {
        let snapshot = self.server.display_snapshot();
        let Some(bounds) = snapshot.virtual_bounds else { return Ok(()); };
        if snapshot.preview.is_some() || !snapshot.heads.iter().any(|head| preferences.contains_key(&head.name)) { return Ok(()); }
        let mut catalog = snapshot.heads;
        let mut heads = DisplayConfiguration::new();
        for head in &mut catalog {
            let Some(preference) = preferences.get(&head.name) else {
                heads.insert(head.id, head.settings());
                continue;
            };
            if !preference.enabled { heads.insert(head.id, None); continue; }
            let mut settings = head.settings().ok_or_else(|| io::Error::other("The virtual monitor has no initial mode"))?;
            if preference.mode.is_some() || preference.refresh_mhz.is_some() {
                let mode = OutputMode { size: preference.mode.map_or(settings.mode.size, |size| size.map(|value| value as i32)),
                    refresh_mhz: preference.refresh_mhz.unwrap_or(settings.mode.refresh_mhz), preferred: false };
                settings.mode = head.modes.iter().find(|candidate| candidate.size == mode.size && candidate.refresh_mhz == mode.refresh_mhz)
                    .cloned().unwrap_or_else(|| { head.modes.push(mode.clone()); mode });
            }
            settings.position = preference.position.unwrap_or(settings.position);
            settings.scale = preference.scale;
            settings.rotation = preference.rotation;
            settings.adaptive_sync = preference.adaptive_sync;
            settings.color = preference.color;
            heads.insert(head.id, Some(settings));
        }
        if !self.server.install_virtual_outputs(catalog, &heads, bounds, false)? {
            return Err(io::Error::other("The saved virtual monitors do not fit this nested window or request an unavailable mode"));
        }
        self.scene_revision = 0;
        self.redraw = true;
        Ok(())
    }

    pub(crate) fn resize_virtual_outputs(&mut self, size: [u32; 2]) -> io::Result<()> {
        let snapshot = self.server.display_snapshot();
        let width = self.size[0] / self.output_count as u32;
        let default = snapshot.heads.len() == self.output_count && snapshot.heads.iter().enumerate().all(|(index, head)| {
            let expected = [if index + 1 == self.output_count { self.size[0] - index as u32 * width } else { width }, self.size[1]];
            head.settings().is_some_and(|settings| settings.mode.size == expected.map(|value| value as i32)
                && settings.position == [(index as u32 * width) as i32, 0] && settings.scale == 1.0
                && settings.rotation == Default::default())
        });
        let fits = snapshot.heads.iter().filter_map(|head| head.settings()).all(|settings| settings.position.iter()
            .zip(settings.logical_size()).zip(size).all(|((origin, extent), bound)| *origin >= 0 && extent > 0
                && i64::from(*origin) + i64::from(extent) <= i64::from(bound)));
        self.size = size;
        self.server.display_bounds(Some(size));
        if default || !fits {
            configure_outputs(&mut self.server, size, self.output_count)?;
            self.invalidate_outputs()?;
            if !default {
                self.send(Input::Feedback("The virtual monitor arrangement no longer fits this nested window. A fitting arrangement is active; saved monitor preferences are unchanged.".into()))?;
            }
        }
        self.redraw = true;
        Ok(())
    }
}
