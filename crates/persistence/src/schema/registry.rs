use crate::{
    models::{
        auth::{AppUserRow, OrganRow, RoleRow, ViewDependencyRow},
        core::{
            CollectionRow, CollectionViewRow, CommandRow, ConfigurationRow, FrequencyRow,
            HistoryRow, QueryRow, RecordRow, SumRow, TransferRow, ViewRow,
        },
        karma::{KarmaConditionRow, KarmaConsequenceRow, KarmaRow},
        sidecars::{
            RecordCommentRow, RecordExtensionRow, RecordLinkRow, RecordResourceRefRow,
            RecordWorklogRow,
        },
    },
    schema::types::{Table, TableSchema},
};

pub fn declared_tables() -> Vec<TableSchema> {
    vec![
        RecordRow::schema(),
        ViewRow::schema(),
        CollectionRow::schema(),
        ConfigurationRow::schema(),
        CollectionViewRow::schema(),
        KarmaConditionRow::schema(),
        KarmaConsequenceRow::schema(),
        KarmaRow::schema(),
        FrequencyRow::schema(),
        CommandRow::schema(),
        TransferRow::schema(),
        SumRow::schema(),
        HistoryRow::schema(),
        QueryRow::schema(),
        RoleRow::schema(),
        AppUserRow::schema(),
        OrganRow::schema(),
        ViewDependencyRow::schema(),
        RecordExtensionRow::schema(),
        RecordLinkRow::schema(),
        RecordCommentRow::schema(),
        RecordWorklogRow::schema(),
        RecordResourceRefRow::schema(),
    ]
}
