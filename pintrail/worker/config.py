from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    redis_host: str = Field(default="localhost", alias="REDIS_HOST")
    redis_port: int = Field(default=6379, alias="REDIS_PORT")
    image_queue_name: str = Field(
        default="artifact-image-processing", alias="IMAGE_QUEUE_NAME"
    )

    image_storage_root: str = Field(default="./data/images", alias="IMAGE_STORAGE_ROOT")

    artifact_service_url: str = Field(
        default="http://localhost:8001", alias="ARTIFACT_SERVICE_URL"
    )
    artifact_api_key: str = Field(default="dev-api-key", alias="ARTIFACT_API_KEY")

    model_config = SettingsConfigDict(
        case_sensitive=False,
        env_file=None,
        extra="ignore",
    )


settings = Settings()
