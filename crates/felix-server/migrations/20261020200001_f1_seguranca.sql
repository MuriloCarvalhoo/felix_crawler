-- F1: endurecimento de segurança (revisões de Rust e de segurança).

-- Auditoria imutável para o papel da aplicação (ADR-018): só INSERT/SELECT.
-- A exclusão de organização é lógica; o CASCADE de FKs roda como dono da tabela.
REVOKE UPDATE, DELETE, TRUNCATE ON auditoria FROM felix_app;

-- Quem criou a organização: limite de organizações ativas por usuário.
ALTER TABLE organizacoes
    ADD COLUMN criado_por uuid REFERENCES usuarios (id) ON DELETE SET NULL;
CREATE INDEX organizacoes_criado_por_idx ON organizacoes (criado_por) WHERE excluida_em IS NULL;

-- Cota diária de convites por organização e revogação dos pendentes de quem
-- perdeu o papel de admin.
CREATE INDEX convites_org_criado_idx ON convites (organizacao_id, criado_em);
CREATE INDEX convites_pendentes_por_idx ON convites (organizacao_id, convidado_por)
    WHERE aceito_em IS NULL AND revogado_em IS NULL;

-- Eventos de autenticação (ADR-021): tabela global, sem RLS e sem endpoint.
-- Nunca guarda senha nem e-mail em claro (só SHA-256 do e-mail normalizado).
CREATE TABLE eventos_autenticacao (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    usuario_id uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    email_hash bytea CHECK (email_hash IS NULL OR length(email_hash) = 32),
    tipo       text NOT NULL CHECK (tipo IN (
                   'login_sucesso', 'login_falha', 'logout', 'senha_trocada', 'senha_redefinida'
               )),
    ip         inet,
    user_agent text CHECK (length(user_agent) <= 512),
    criado_em  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX eventos_autenticacao_usuario_idx ON eventos_autenticacao (usuario_id, criado_em DESC);
CREATE INDEX eventos_autenticacao_email_idx ON eventos_autenticacao (email_hash, criado_em DESC);
REVOKE UPDATE, DELETE, TRUNCATE ON eventos_autenticacao FROM felix_app;

-- Purga de e-mails antigos da caixa de saída (email::purgar_antigos).
CREATE INDEX emails_saida_criado_idx ON emails_saida (criado_em);
