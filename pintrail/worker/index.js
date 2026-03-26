const fs = require('fs/promises');
const path = require('path');
const { Worker } = require('bullmq');
const { Pool } = require('pg');
const sharp = require('sharp');

const redisConnection = {
  host: process.env.REDIS_HOST ?? 'localhost',
  port: Number(process.env.REDIS_PORT ?? 6379),
};

const queueName = process.env.IMAGE_QUEUE_NAME ?? 'artifact-image-processing';
const imageRoot = process.env.IMAGE_STORAGE_ROOT ?? '/data/images';
const processedDir = path.join(imageRoot, 'processed');
const pool = new Pool({
  host: process.env.DB_HOST ?? 'localhost',
  port: Number(process.env.DB_PORT ?? 5432),
  database: process.env.DB_NAME ?? 'pintrail',
  user: process.env.DB_USER ?? 'pintrail',
  password: process.env.DB_PASSWORD ?? 'pintrail',
});

async function ensureDirectories() {
  await fs.mkdir(processedDir, { recursive: true });
}

async function processImage(job) {
  const { imageId, originalPath } = job.data;
  const client = await pool.connect();

  try {
    await client.query(
      `
        update artifact_images
        set status = 'processing', "errorMessage" = null, "updatedAt" = now()
        where id = $1
      `,
      [imageId],
    );

    const originalAbsolutePath = path.join(imageRoot, originalPath);
    const processedFilename = `${imageId}.webp`;
    const processedRelativePath = path.join('processed', processedFilename);
    const processedAbsolutePath = path.join(imageRoot, processedRelativePath);

    const result = await sharp(originalAbsolutePath)
      .rotate()
      .resize({
        width: 1600,
        height: 1600,
        fit: 'inside',
        withoutEnlargement: true,
      })
      .webp({ quality: 82 })
      .toFile(processedAbsolutePath);

    await client.query(
      `
        update artifact_images
        set
          status = 'processed',
          "processedFilename" = $2,
          "processedMimeType" = 'image/webp',
          width = $3,
          height = $4,
          "errorMessage" = null,
          "updatedAt" = now()
        where id = $1
      `,
      [imageId, processedRelativePath, result.width ?? null, result.height ?? null],
    );
  } catch (error) {
    await client.query(
      `
        update artifact_images
        set status = 'failed', "errorMessage" = $2, "updatedAt" = now()
        where id = $1
      `,
      [imageId, error instanceof Error ? error.message : 'Image processing failed.'],
    );
    throw error;
  } finally {
    client.release();
  }
}

async function main() {
  await ensureDirectories();

  const worker = new Worker(queueName, processImage, {
    connection: redisConnection,
    concurrency: 3,
  });

  worker.on('completed', job => {
    console.log(`Processed image job ${job.id}`);
  });

  worker.on('failed', (job, error) => {
    console.error(`Image job ${job?.id ?? 'unknown'} failed:`, error.message);
  });

  const shutdown = async () => {
    await worker.close();
    await pool.end();
    process.exit(0);
  };

  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);

  console.log(`Image worker listening on queue "${queueName}"`);
}

main().catch(error => {
  console.error('Failed to start image worker:', error);
  process.exit(1);
});
