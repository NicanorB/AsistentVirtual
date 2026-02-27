use rocket::{
    Route, get, launch, routes,
    serde::{Serialize, json::Json},
};
use rocket_db_pools::{
    Connection, Database,
    diesel::{self, QueryResult},
};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct Settings {
    name: String,
    status: String,
}

/*
#[derive(Database)]
#[database("asistentvirtual-db")]
pub struct Db(diesel::PgPool);

#[get("/get_users")]
async fn get_users(mut db: Connection<Db>) -> QueryResult<String> {
    Ok("test".to_string())
}
*/

#[get("/get_users")]
async fn get_users() -> String {
    "test".to_string()
}

#[get("/get_settings")]
async fn get_settings() -> Json<Settings> {
    Json(Settings {
        name: "AsistentVirtual".to_string(),
        status: "OK".to_string(),
    })
}

pub fn routes() -> Vec<Route> {
    routes![get_users, get_settings]
}

#[launch]
fn rocket() -> _ {
    /*
    rocket::build().attach(Db::init()).mount("/api", routes())
    */
    rocket::build().mount("/api", routes())
}
