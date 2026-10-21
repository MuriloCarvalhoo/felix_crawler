-- Executado uma única vez, na criação do volume, pelo superusuário postgres.
-- felix_app: papel sem login e sem BYPASSRLS usado em SET LOCAL ROLE pelas
-- transações de tenant. felix: usuário da aplicação, dono do banco felix.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'felix_app') THEN
    CREATE ROLE felix_app NOLOGIN NOBYPASSRLS;
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'felix') THEN
    CREATE ROLE felix LOGIN PASSWORD 'felix' NOSUPERUSER NOCREATEROLE NOBYPASSRLS;
  END IF;
END
$$;

GRANT felix_app TO felix;

CREATE DATABASE felix OWNER felix;

\connect felix
-- citext precisa existir antes da primeira migração (o dono do banco não é superusuário).
CREATE EXTENSION IF NOT EXISTS citext;
