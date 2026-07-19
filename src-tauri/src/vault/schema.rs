//! MongrelDB kit schema for the PromptSack index.

use mongreldb_kit::{Column, ColumnType, Index, IndexKind, Schema, Table};

pub fn vault_schema() -> Schema {
    let folders = Table {
        id: 1,
        name: "folders".into(),
        columns: vec![
            Column::new(1, "id", ColumnType::Text),
            Column::new(2, "name", ColumnType::Text),
            Column::new(3, "locked", ColumnType::Bool),
            Column::new(4, "created_at", ColumnType::Text),
            Column::new(5, "updated_at", ColumnType::Text),
        ],
        primary_key: vec!["id".into()],
        indexes: vec![Index {
            name: "idx_folders_name".into(),
            columns: vec!["name".into()],
            unique: false,
            kind: IndexKind::Bitmap,
        }],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
    };

    let smart_folders = Table {
        id: 2,
        name: "smart_folders".into(),
        columns: vec![
            Column::new(1, "id", ColumnType::Text),
            Column::new(2, "name", ColumnType::Text),
            Column::new(3, "query_json", ColumnType::Text),
            Column::new(4, "created_at", ColumnType::Text),
            Column::new(5, "updated_at", ColumnType::Text),
        ],
        primary_key: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
    };

    let mut emb = Column::new(13, "embedding", ColumnType::Embedding);
    emb.embedding_dim = Some(384);
    emb.nullable = true;

    let mut folder_id = Column::new(6, "folder_id", ColumnType::Text);
    folder_id.nullable = true;

    let prompt_index = Table {
        id: 3,
        name: "prompt_index".into(),
        columns: vec![
            Column::new(1, "id", ColumnType::Text),
            Column::new(2, "title", ColumnType::Text),
            Column::new(3, "body_fts", ColumnType::Text),
            Column::new(4, "notes_fts", ColumnType::Text),
            Column::new(5, "tags_json", ColumnType::Text),
            folder_id,
            Column::new(7, "favorite", ColumnType::Bool),
            Column::new(8, "locked", ColumnType::Bool),
            Column::new(9, "content_encrypted", ColumnType::Bool),
            Column::new(10, "md_path", ColumnType::Text),
            Column::new(11, "created_at", ColumnType::Text),
            Column::new(12, "updated_at", ColumnType::Text),
            emb,
        ],
        primary_key: vec!["id".into()],
        indexes: vec![
            Index {
                name: "idx_pi_title".into(),
                columns: vec!["title".into()],
                unique: false,
                kind: IndexKind::Fm,
            },
            Index {
                name: "idx_pi_body".into(),
                columns: vec!["body_fts".into()],
                unique: false,
                kind: IndexKind::Fm,
            },
            Index {
                name: "idx_pi_notes".into(),
                columns: vec!["notes_fts".into()],
                unique: false,
                kind: IndexKind::Fm,
            },
            Index {
                name: "idx_pi_tags".into(),
                columns: vec!["tags_json".into()],
                unique: false,
                kind: IndexKind::Fm,
            },
            Index {
                name: "idx_pi_embedding".into(),
                columns: vec!["embedding".into()],
                unique: false,
                kind: IndexKind::Ann,
            },
        ],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
    };

    let ui_state = Table {
        id: 4,
        name: "ui_state".into(),
        columns: vec![
            Column::new(1, "key", ColumnType::Text),
            Column::new(2, "value", ColumnType::Text),
        ],
        primary_key: vec!["key".into()],
        indexes: vec![],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
    };

    let meta = Table {
        id: 5,
        name: "meta".into(),
        columns: vec![
            Column::new(1, "key", ColumnType::Text),
            Column::new(2, "value", ColumnType::Text),
        ],
        primary_key: vec!["key".into()],
        indexes: vec![],
        foreign_keys: vec![],
        unique_constraints: vec![],
        check_constraints: vec![],
    };

    Schema::new(vec![
        folders,
        smart_folders,
        prompt_index,
        ui_state,
        meta,
    ])
    .expect("valid vault schema")
}
