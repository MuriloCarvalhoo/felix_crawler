// Recompila quando uma migração muda (sqlx::migrate! embute os arquivos).
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
