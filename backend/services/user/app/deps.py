import redis.asyncio as aioredis
from sqlmodel import SQLModel
from sqlmodel.ext.asyncio.session import AsyncSession
from sqlalchemy.ext.asyncio import create_async_engine, async_sessionmaker
from libs.python.common.settings import settings

# Async DB engine for SQLModel
# Psycopg3 async driver: postgresql+psycopg -> add '+asyncpg' or async flag?
# We’ll use psycopg async dialect:
ASYNC_DB_URL = str(settings.database_url).replace(
    "postgresql+psycopg://", "postgresql+psycopg://"
)

engine = create_async_engine(ASYNC_DB_URL, echo=False, pool_pre_ping=True)
SessionLocal = async_sessionmaker(engine, expire_on_commit=False, class_=AsyncSession)


async def init_db() -> None:
    async with engine.begin() as conn:
        await conn.run_sync(SQLModel.metadata.create_all)


# Redis connection (async)
redis_client = aioredis.from_url(str(settings.redis_url), decode_responses=True)


async def get_session() -> AsyncSession:
    async with SessionLocal() as session:
        yield session
