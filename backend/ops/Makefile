SHELL := /bin/bash

up:
	@cp -n .env.example .env || true
	docker compose --env-file .env up -d --build

down:
	docker compose --env-file .env down

logs:
	docker compose --env-file .env logs -f --tail=200

ps:
	docker compose --env-file .env ps

pss:
	@docker compose ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"

restart:
	docker compose --env-file .env restart

pg-attach:
	docker compose exec postgres psql -U pintrails -d pintrails

pg-users:
	@docker compose exec postgres \
  	psql -U pintrails -d pintrails \
  	-c "SELECT * FROM public.users ORDER BY id;"

pg-users-csv:
	@docker compose exec postgres \
  	psql -q -U pintrails -d pintrails \
  	-c "\pset format csv" -c "SELECT * FROM public.users ORDER BY id;"

recreate:
	docker compose --env-file .env down -v
	docker compose --env-file .env up -d --build
