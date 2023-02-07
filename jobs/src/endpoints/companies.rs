use crate::schemas::companies::{Company, CreateCompany, UpdateCompany};

use super::Tags;
use entity::jobs_companies;
use poem::error::InternalServerError;
use poem::Result;
use poem_openapi::{param::Path, payload::Json, ApiResponse, OpenApi};
use sea_orm::{
    ActiveModelTrait, ActiveValue, DatabaseConnection, EntityTrait, ModelTrait, Set, Unchanged,
};
use uuid::Uuid;

pub struct Companies {
    pub db: DatabaseConnection,
}

#[OpenApi(tag = "Tags::Companies")]
impl Companies {
    /// List all companies.
    #[oai(path = "/companies", method = "get")]
    async fn list_companies(&self) -> Result<Json<Vec<Company>>> {
        Ok(Json(
            jobs_companies::Entity::find()
                .all(&self.db)
                .await
                .map_err(InternalServerError)?
                .into_iter()
                .map(Into::into)
                .collect(),
        ))
    }

    /// Create a company.
    #[oai(path = "/companies", method = "post")]
    async fn create_company(&self, data: Json<CreateCompany>) -> Result<Json<Company>> {
        Ok(Json(
            jobs_companies::ActiveModel {
                id: Set(Uuid::new_v4()),
                name: Set(data.0.name),
                description: Set(data.0.description),
                website: Set(data.0.website),
                youtube_video: Set(data.0.youtube_video),
                twitter_handle: Set(data.0.twitter_handle),
                instagram_handle: Set(data.0.instagram_handle),
                logo_url: Set(data.0.logo_url),
            }
            .insert(&self.db)
            .await
            .map_err(InternalServerError)?
            .into(),
        ))
    }

    /// Update a company.
    #[oai(path = "/companies/:company_id", method = "patch")]
    async fn update_company(
        &self,
        company_id: Path<Uuid>,
        data: Json<UpdateCompany>,
    ) -> Result<UpdateResponse> {
        Ok(match self.get_company(company_id.0).await? {
            Some(company) => UpdateResponse::Ok(Json(
                jobs_companies::ActiveModel {
                    id: Unchanged(company.id),
                    name: update(company.name, data.0.name),
                    description: update(company.description, data.0.description),
                    website: update(company.website, data.0.website),
                    youtube_video: update(company.youtube_video, data.0.youtube_video),
                    twitter_handle: update(company.twitter_handle, data.0.twitter_handle),
                    instagram_handle: update(company.instagram_handle, data.0.instagram_handle),
                    logo_url: update(company.logo_url, data.0.logo_url),
                }
                .update(&self.db)
                .await
                .map_err(InternalServerError)?
                .into(),
            )),
            None => UpdateResponse::NotFound,
        })
    }

    /// Delete a company.
    #[oai(path = "/companies/:company_id", method = "delete")]
    async fn delete_company(&self, company_id: Path<Uuid>) -> Result<DeleteResponse> {
        Ok(match self.get_company(company_id.0).await? {
            Some(company) => {
                company
                    .delete(&self.db)
                    .await
                    .map_err(InternalServerError)?;
                DeleteResponse::Ok
            }
            None => DeleteResponse::NotFound,
        })
    }
}

#[derive(ApiResponse)]
enum UpdateResponse {
    /// Company has been updated successfully
    #[oai(status = 200)]
    Ok(Json<Company>),
    /// Could not find company
    #[oai(status = 404)]
    NotFound,
}

#[derive(ApiResponse)]
enum DeleteResponse {
    /// Company has been deleted successfully
    #[oai(status = 200)]
    Ok,
    /// Could not find company
    #[oai(status = 404)]
    NotFound,
}

impl Companies {
    async fn get_company(&self, company_id: Uuid) -> Result<Option<jobs_companies::Model>> {
        jobs_companies::Entity::find_by_id(company_id)
            .one(&self.db)
            .await
            .map_err(InternalServerError)
    }
}

fn update<T: Into<sea_orm::Value>>(old: T, new: Option<T>) -> ActiveValue<T> {
    if let Some(new) = new {
        Set(new)
    } else {
        Unchanged(old)
    }
}
