#![allow(missing_docs, unused_import_braces)]

table! {
    people (id) {
        id -> Int4,
        wmbid -> Nullable<Varchar>,
        snowflake -> Nullable<Int8>,
        active -> Bool,
        data -> Nullable<Jsonb>,
        version -> Int4,
        apikey -> Nullable<Varchar>,
        discorddata -> Nullable<Jsonb>,
    }
}
