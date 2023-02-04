pub use sea_orm_migration::prelude::*;

pub struct Migrator;

mod m20230204_171617_create_companies_table;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20230204_171617_create_companies_table::Migration)]
    }
}
