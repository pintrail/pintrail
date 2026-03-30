import os
from dotenv import load_dotenv

load_dotenv()


class Settings:
    db_host: str = os.getenv("DB_HOST", "localhost")
    db_port: int = int(os.getenv("DB_PORT", "5432"))
    db_user: str = os.getenv("DB_USER", "pintrail")
    db_password: str = os.getenv("DB_PASSWORD", "pintrail")
    db_name: str = os.getenv("DB_NAME", "pintrail")

    port: int = int(os.getenv("PORT", "8000"))
    env: str = os.getenv("ENV", "development")

    auth_admin_email: str = os.getenv("AUTH_ADMIN_EMAIL", "")
    auth_admin_password: str = os.getenv("AUTH_ADMIN_PASSWORD", "")
    auth_session_ttl_hours: int = int(os.getenv("AUTH_SESSION_TTL_HOURS", "24"))

    redis_host: str = os.getenv("REDIS_HOST", "localhost")
    redis_port: int = int(os.getenv("REDIS_PORT", "6379"))
    image_queue_name: str = os.getenv("IMAGE_QUEUE_NAME", "artifact-image-processing")

    image_storage_root: str = os.getenv("IMAGE_STORAGE_ROOT", "./data/images")

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
