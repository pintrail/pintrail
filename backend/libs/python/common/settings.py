# libs/python/common/settings.py
from pydantic_settings import BaseSettings, SettingsConfigDict
from pydantic import Field


class Settings(BaseSettings):
    service_name: str = Field(default="service", alias="SERVICE_NAME")
    database_url: str = Field(alias="DATABASE_URL")
    redis_url: str = Field(default="redis://redis:6379/0", alias="REDIS_URL")
    uvicorn_host: str = Field(default="0.0.0.0", alias="UVICORN_HOST")
    uvicorn_port: int = Field(default=8000, alias="UVICORN_PORT")

    # pydantic-settings v2 style config
    model_config = SettingsConfigDict(
        case_sensitive=False,
        env_file=None,  # rely on docker environment
        extra="ignore",
    )


settings = Settings()
