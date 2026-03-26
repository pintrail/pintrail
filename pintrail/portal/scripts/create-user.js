const { randomBytes, scryptSync } = require('crypto');
const { Client } = require('pg');

function parseArgs(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 1) {
    const entry = argv[index];
    if (!entry.startsWith('--')) {
      continue;
    }

    const key = entry.slice(2);
    const value = argv[index + 1];
    args[key] = value;
    index += 1;
  }
  return args;
}

function normalizeEmail(email) {
  return email.trim().toLowerCase();
}

function hashPassword(password) {
  const salt = randomBytes(16).toString('hex');
  const derivedKey = scryptSync(password, salt, 64);
  return `${salt}:${derivedKey.toString('hex')}`;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const email = args.email ? normalizeEmail(args.email) : null;
  const password = args.password ?? null;
  const role = args.role ?? 'viewer';
  const roles = new Set(['admin', 'editor', 'viewer']);

  if (!email || !password) {
    throw new Error('Usage: npm run user:create -- --email user@example.com --password secret123 --role editor');
  }

  if (password.length < 8) {
    throw new Error('Password must be at least 8 characters long.');
  }

  if (!roles.has(role)) {
    throw new Error('Role must be one of: admin, editor, viewer.');
  }

  const client = new Client({
    host: process.env.DB_HOST ?? 'localhost',
    port: Number(process.env.DB_PORT ?? 5432),
    database: process.env.DB_NAME ?? 'pintrail',
    user: process.env.DB_USER ?? 'pintrail',
    password: process.env.DB_PASSWORD ?? 'pintrail',
  });

  await client.connect();

  try {
    const existingUser = await client.query('select id from users where email = $1', [email]);
    if (existingUser.rowCount) {
      throw new Error(`A user with email ${email} already exists.`);
    }

    await client.query(
      `
        insert into users (email, "passwordHash", role, "isActive")
        values ($1, $2, $3, true)
      `,
      [email, hashPassword(password), role],
    );

    console.log(`Created ${role} user ${email}`);
  } finally {
    await client.end();
  }
}

main().catch(error => {
  console.error(error.message);
  process.exit(1);
});
