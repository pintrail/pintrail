# Seed data

SQL files that load sample content into a running database. They are not
migrations: nothing runs them automatically, and they are safe to run more
than once.

| File | What it does |
|---|---|
| `examples.sql` | Loads two real campus buildings, each with child artifacts, tags, and source links, as worked examples for new authors. Every name starts with `Example:`. They load **approved** and with no owner, so only an admin can change or delete them. |
| `remove-examples.sql` | Deletes them again, children, tags, and links included. Their entries in `artifact_history` stay, by design. |

The examples show what a good artifact looks like: what it is, where it is,
why it matters, how it works, real numbers, and a source. They also show the
hierarchy: the building has coordinates, and its features have none of their
own because they inherit the building's location.

## Running them on the server

From `~/pintrail/backend` (Compose reads `COMPOSE_FILE` from `.env`):

```sh
docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/examples.sql
docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/remove-examples.sql
```

No redeploy is needed: these write straight to the database, and the Studio
shows the result on the next page load. Because they run outside the Studio,
their changes appear in each artifact's history as made by "system". The examples need the tags and links
tables, so deploy that release (which runs its migration) before loading them.

Re-running `examples.sql` skips anything already there, so it won't update
examples loaded from an older version of the file. To refresh them, run
`remove-examples.sql` and then `examples.sql`.
