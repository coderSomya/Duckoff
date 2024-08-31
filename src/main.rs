mod ws;
use actix_files as fs;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use chrono::{NaiveDateTime, TimeZone};
use duckdb::{Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Transaction {
    id: String,
    sender: String,
    reciever: String,
    amount: i32,
    #[serde(with = "date_format")]
    time: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
struct FilterParams {
    sender: Option<String>,
    reciever: Option<String>,
    id: Option<String>,
    min_amount: Option<i32>,
    max_amount: Option<i32>,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NewTransaction {
    id: String,
    sender: String,
    reciever: String,
    amount: i32,
    #[serde(with = "date_format")]
    time: NaiveDateTime,
}

mod date_format {
    use chrono::{NaiveDateTime, TimeZone};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(dt: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&dt.format(FORMAT).to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDateTime::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

async fn get_transactions(params: web::Query<FilterParams>) -> impl Responder {
    let mut conn = Connection::open("transactions.db").expect("could not open connection");

    conn.execute_batch(
        r"
        CREATE TABLE IF NOT EXISTS transactions (
            id VARCHAR PRIMARY KEY,
            sender VARCHAR,
            reciever VARCHAR,
            amount INTEGER,
            time VARCHAR
        )",
    )
    .expect("Failed to create table");

    let mut query =
        String::from("SELECT id, sender, reciever, amount, time FROM transactions WHERE 1=1");

    if let Some(ref sender) = params.sender {
        query.push_str(&format!(" AND sender = '{}'", sender));
    }
    if let Some(ref reciever) = params.reciever {
        query.push_str(&format!(" AND reciever = '{}'", reciever));
    }
    if let Some(ref id) = params.id {
        query.push_str(&format!(" AND id = '{}'", id));
    }
    if let Some(min_amount) = params.min_amount {
        query.push_str(&format!(" AND amount >= {}", min_amount));
    }
    if let Some(max_amount) = params.max_amount {
        query.push_str(&format!(" AND amount <= {}", max_amount));
    }
    if let Some(ref start_date) = params.start_date {
        query.push_str(&format!(" AND time >= '{}'", start_date));
    }
    if let Some(ref end_date) = params.end_date {
        query.push_str(&format!(" AND time <= '{}'", end_date));
    }

    let mut stmt = conn.prepare(&query).expect("Failed to prepare statement");

    let transaction_iter = stmt
        .query_map([], |row| {
            let time_str: String = row.get(4)?;
            let time = NaiveDateTime::parse_from_str(&time_str, "%Y-%m-%d %H:%M:%S")
                .expect("could not parse string..");

            Ok(Transaction {
                id: row.get(0)?,
                sender: row.get(1)?,
                reciever: row.get(2)?,
                amount: row.get(3)?,
                time,
            })
        })
        .expect("Failed to execute query");

    let transactions: Vec<Transaction> = transaction_iter
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect transactions");

    HttpResponse::Ok().json(transactions)
}

async fn add_transaction(
    new_transaction: web::Json<NewTransaction>,
    ws_server: web::Data<ws::WsServer>,
) -> impl Responder {
    let mut conn = Connection::open("transactions.db").expect("could not open connection");

    conn.execute_batch(
        r"
        CREATE TABLE IF NOT EXISTS transactions (
            id VARCHAR PRIMARY KEY,
            sender VARCHAR,
            reciever VARCHAR,
            amount INTEGER,
            time VARCHAR
        )",
    )
    .expect("Failed to create table");

    conn.execute(
        "INSERT INTO transactions (id, sender, reciever, amount, time)
        VALUES (?,?,?,?,?)",
        &[
            new_transaction.id.as_str(),
            new_transaction.sender.as_str(),
            new_transaction.reciever.as_str(),
            &new_transaction.amount.to_string(),
            &new_transaction.time.format("%Y-%m-%d %H:%M:%S").to_string(),
        ],
    )
    .expect("Failed to insert transaction");

    let message = serde_json::to_string(&*new_transaction).unwrap();
    ws_server.broadcast(message);

    HttpResponse::Ok().body("Transaction was successfull..!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let ws_server = web::Data::new(ws::WsServer::new());

    HttpServer::new(move || {
        App::new()
            .app_data(ws_server.clone())
            .wrap(Logger::default())
            .route("/api/transactions", web::get().to(get_transactions))
            .route("/api/transactions", web::post().to(add_transaction))
            .route("/ws/", web::get().to(ws::ws_index))
            .service(fs::Files::new("/", "./static").index_file("index.html"))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
