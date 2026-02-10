use {crate::SAideApp, egui_action::ActionArgs, egui_action_macro::impl_action};

#[impl_action]
impl SAideApp {
    pub fn create_profile(&mut self, profile_name: &str) {
        let state = self.state();
        let Some(mapper) = state.keyboard_mapper.as_ref() else {
            return;
        };

        mapper.create_profile(&state.device_serial, state.device_orientation, profile_name);
    }

    pub fn delete_profile(&mut self) {
        if let Some(mapper) = self.state().keyboard_mapper.as_ref() {
            mapper.delete_active_profile();
        }
    }

    pub fn save_profile_as(&mut self, new_name: &str) {
        let state = self.state();
        let Some(mapper) = state.keyboard_mapper.as_ref() else {
            return;
        };

        if let Some(active_profile) = mapper.get_active_profile() {
            mapper.save_profile_as(&active_profile.name, new_name);
        }
    }

    pub fn rename_profile(&mut self, new_name: &str) {
        let Some(mapper) = self.state().keyboard_mapper.as_ref() else {
            return;
        };

        mapper.rename_active_profile(new_name);
    }

    pub fn switch_profile(&mut self, idx: usize) {
        let Some(mapper) = self.state().keyboard_mapper.as_ref() else {
            return;
        };

        if idx < mapper.get_avail_profiles().len() {
            mapper.switch_to_profile_by_name(idx);
        }
    }

    pub fn next_profile(&mut self) {
        let Some(mapper) = self.state().keyboard_mapper.as_ref() else {
            return;
        };

        let current_idx = mapper.get_active_profile_idx().unwrap_or(0);
        let profiles_len = mapper.get_avail_profiles().len();
        let next_idx = (current_idx + 1) % profiles_len;
        mapper.switch_to_profile_by_name(next_idx);
    }

    pub fn prev_profile(&mut self) {
        let Some(mapper) = self.state().keyboard_mapper.as_ref() else {
            return;
        };

        let current_idx = mapper.get_active_profile_idx().unwrap_or(0);
        let profiles_len = mapper.get_avail_profiles().len();
        let prev_idx = (current_idx + profiles_len - 1) % profiles_len;
        mapper.switch_to_profile_by_name(prev_idx);
    }
}
