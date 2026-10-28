use super::*;
use smithay::{
    delegate_xdg_activation,
    wayland::xdg_activation::{
        XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
    },
};
use std::time::Duration;

struct InputEpoch(u64);

impl State {
    fn prune_activation_tokens(&mut self) {
        let epoch = self.input_epoch;
        self.activation.retain_tokens(|_, data| {
            data.timestamp.elapsed() < Duration::from_secs(5)
                && data
                    .user_data
                    .get::<InputEpoch>()
                    .is_some_and(|value| value.0 == epoch)
        });
    }
}

impl XdgActivationHandler for State {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.activation
    }

    fn token_created(&mut self, _: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        self.prune_activation_tokens();
        if self.lock.owner.is_some()
            || self.activation.tokens().count() >= 128
            || data.app_id.as_ref().is_some_and(|id| id.len() > 4096)
        {
            return false;
        }
        let Some((serial, seat)) = data.serial else {
            return false;
        };
        let Some(client) = data.client_id else {
            return false;
        };
        let valid = Seat::<State>::from_resource(&seat) == Some(self.seat.clone())
            && self.authorized_client_input(serial, &client, data.surface.as_ref());
        if valid {
            data.user_data
                .insert_if_missing_threadsafe(|| InputEpoch(self.input_epoch));
        }
        valid
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        self.activation.remove_token(&token);
        if self.lock.owner.is_some()
            || data.timestamp.elapsed() >= Duration::from_secs(5)
            || data
                .user_data
                .get::<InputEpoch>()
                .is_none_or(|epoch| epoch.0 != self.input_epoch)
        {
            return;
        }
        let Some(id) = self.id(&surface) else {
            return;
        };
        if self.activations.len() < 64 {
            let initial = self.windows.get(&id).is_some_and(|window| !window.mapped || data.timestamp <= window.created);
            self.activations.retain(|(pending, _, _, _)| *pending != id);
            self.activations
                .push_back((id, data.timestamp, self.input_epoch, initial));
        }
    }
}

impl Server {
    pub(crate) fn launch_activation_token(&mut self) -> io::Result<XdgActivationToken> {
        if self.locked() {
            return Err(io::Error::other(
                "Foreground activation is unavailable while locked",
            ));
        }
        self.state.prune_activation_tokens();
        if self.state.activation.tokens().count() >= 128 {
            return Err(io::Error::other("Park activation token limit reached"));
        }
        let epoch = self.state.input_epoch;
        let (token, data) = self.state.activation.create_external_token(None);
        data.user_data
            .insert_if_missing_threadsafe(|| InputEpoch(epoch));
        Ok(token.clone())
    }

    pub fn activations(&mut self) -> Vec<SurfaceId> {
        self.activation_requests().into_iter().map(|(surface, _)| surface).collect()
    }

    pub fn activation_requests(&mut self) -> Vec<(SurfaceId, bool)> {
        if self.locked() {
            self.state.activations.clear();
            return Vec::new();
        }
        let mut ready = Vec::new();
        self.state.activations.retain(|(id, created, epoch, initial)| {
            if created.elapsed() >= Duration::from_secs(5) || *epoch != self.state.input_epoch {
                return false;
            }
            let Some(window) = self.state.windows.get(id) else {
                return false;
            };
            if window.mapped {
                ready.push((*id, *initial));
                false
            } else {
                true
            }
        });
        ready
    }

    pub fn native_input(&mut self) {
        self.state.input_epoch = self.state.input_epoch.saturating_add(1);
    }
}

delegate_xdg_activation!(State);
