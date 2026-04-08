from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    db_host: str = Field(default="localhost", alias="DB_HOST")
    db_port: int = Field(default=5432, alias="DB_PORT")
    db_user: str = Field(default="pintrail", alias="DB_USER")
    db_password: str = Field(default="pintrail", alias="DB_PASSWORD")
    db_name: str = Field(default="pintrail", alias="DB_NAME")

    redis_host: str = Field(default="localhost", alias="REDIS_HOST")
    redis_port: int = Field(default=6379, alias="REDIS_PORT")
    image_queue_name: str = Field(default="artifact-image-processing", alias="IMAGE_QUEUE_NAME")

    image_storage_root: str = Field(default="./data/images", alias="IMAGE_STORAGE_ROOT")

    model_config = SettingsConfigDict(
        case_sensitive=False,
        env_file=None,
        extra="ignore",
    )

    @property
    def database_url(self) -> str:
        return (
            f"postgresql+asyncpg://{self.db_user}:{self.db_password}"
            f"@{self.db_host}:{self.db_port}/{self.db_name}"
        )


settings = Settings()
