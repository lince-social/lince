pub struct KarmaFilters {
    pub condition_record_id: Option<u32>,
}

impl KarmaFilters {
    pub fn new(condition_record_id: Option<u32>) -> Self {
        Self {
            condition_record_id,
        }
    }
}

impl Default for KarmaFilters {
    fn default() -> Self {
        Self {
            condition_record_id: None,
        }
    }
}
