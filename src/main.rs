use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use rustdds::with_key::Sample;
use rustdds::*;
use serde::{Deserialize, Serialize};
use sled;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use with_key::DataWriter;

type DataWriterState = Arc<Mutex<DataWriter<SensorConfig>>>;
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SensorConfig {
    sensor_type: String,
    frequency: u32,
    power: u32,
    squelti: u32,
}
impl Keyed for SensorConfig {
    type K = String;
    fn key(&self) -> Self::K {
        self.sensor_type.clone()
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SensorStatus {
    sensor_type: String,
    frequency: u32,
    power: u32,
    squelti: u32,
}
impl Keyed for SensorStatus {
    type K = String;
    fn key(&self) -> Self::K {
        self.sensor_type.clone()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // dds
    let domain_participant = DomainParticipantBuilder::new(0).build()?;
    let qos = QosPolicyBuilder::new().build();
    let topic_sensor_config = domain_participant
        .create_topic(
            "SensorConfig".to_string(),
            "Topic: SensorConfig".to_string(),
            &qos,
            TopicKind::WithKey,
        )
        .unwrap();
    let topic_name = "SensorStatus".to_string();
    let topic_sensor_status = domain_participant
        .create_topic(
            topic_name.clone(),
            "Topic: SensorStatus".to_string(),
            &qos,
            TopicKind::WithKey,
        )
        .unwrap();
    let publisher = domain_participant.create_publisher(&qos).unwrap();
    let writer = publisher
        .create_datawriter_cdr::<SensorConfig>(&topic_sensor_config, Some(qos.clone()))
        .unwrap();
    let subscriber = domain_participant.create_subscriber(&qos).unwrap();
    let reader = subscriber
        .create_datareader_cdr::<SensorStatus>(&topic_sensor_status, Some(qos.clone()))
        .unwrap();
    let mut async_reader = reader.async_sample_stream();

    // db
    let db = sled::open(topic_name).unwrap();
    let db_status = db.open_tree("status");
    let db_status_sub = db_status.clone().unwrap();
    let db_status_axum = db_status.clone().unwrap();

    // state
    let state = Arc::new(Mutex::new(writer));

    // background subscriber
    tokio::spawn(async move {
        println!("subscriber start");
        while let Some(Ok(Sample::Value(v))) = &mut async_reader.next().await {
            if let Ok(json) = serde_json::to_string(&v) {
                db_status_sub.insert(v.key(), json.as_bytes()).unwrap();
                println!("subscribe: {:?}", v);
            }
        }
    });

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .route("/sensor/config", put(put_handler_sensor_config))
        .with_state(state)
        .route("/sensor/status", get(get_handler_sensor_status))
        .with_state(db_status_axum);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("localhost:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "DDS Sample Application"
}

async fn put_handler_sensor_config(
    State(writer): State<DataWriterState>,
    Json(payload): Json<SensorConfig>,
) -> (StatusCode, Json<SensorConfig>) {
    //let dds_writer = &mut writer.lock().await;
    let dds_writer = &mut writer.lock().await;
    if let Ok(_) = dds_writer.async_write(payload.clone(), None).await {
        (StatusCode::OK, Json(payload))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SensorConfig {
                ..Default::default()
            }),
        )
    }
}

async fn get_handler_sensor_status(
    State(db_tree): State<sled::Tree>,
) -> (StatusCode, Json<SensorConfig>) {
    if let Ok(Some(value)) = db_tree.get("radio") {
        let deser_json = serde_json::from_slice(&value).unwrap();
        (StatusCode::OK, Json(deser_json))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SensorConfig {
                ..Default::default()
            }),
        )
    }
}
