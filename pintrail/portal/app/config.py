from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    db_host: str = Field(default="localhost", alias="DB_HOST")
    db_port: int = Field(default=5432, alias="DB_PORT")
    db_user: str = Field(default="pintrail", alias="DB_USER")
    db_password: str = Field(default="pintrail", alias="DB_PASSWORD")
    db_name: str = Field(default="pintrail", alias="DB_NAME")

    port: int = Field(default=8000, alias="PORT")
    env: str = Field(default="development", alias="ENV")

    auth_admin_email: str = Field(default="", alias="AUTH_ADMIN_EMAIL")
    auth_admin_password: str = Field(default="", alias="AUTH_ADMIN_PASSWORD")
    auth_session_ttl_hours: int = Field(default=24, alias="AUTH_SESSION_TTL_HOURS")

    artifact_service_url: str = Field(
        default="http://localhost:8001", alias="ARTIFACT_SERVICE_URL"
    )
    artifact_api_key: str = Field(default="dev-api-key", alias="ARTIFACT_API_KEY")
    image_storage_root: str = Field(default="/data/images", alias="IMAGE_STORAGE_ROOT")

    smtp_host: str = Field(default="", alias="SMTP_HOST")
    smtp_port: int = Field(default=587, alias="SMTP_PORT")
    smtp_user: str = Field(default="", alias="SMTP_USER")
    smtp_password: str = Field(default="", alias="SMTP_PASSWORD")
    smtp_from: str = Field(default="noreply@pintrail.app", alias="SMTP_FROM")
    notify_email_enabled: bool = Field(default=False, alias="NOTIFY_EMAIL_ENABLED")

    model_config = SettingsConfigDict(
        case_sensitive=False,
        env_file=None,
        extra="ignore",
    )

    @property
    def is_production(self) -> bool:
        return self.env == "production"

    @property
    def database_url(self) -> str:
        return (
            f"postgresql+asyncpg://{self.db_user}:{self.db_password}"
            f"@{self.db_host}:{self.db_port}/{self.db_name}"
        )


settings = Settings()
