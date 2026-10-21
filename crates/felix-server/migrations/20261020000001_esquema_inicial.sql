-- Esquema inicial do Felix SaaS (docs/saas/02-arquitetura.md).
-- Requisitos: PostgreSQL 18 (uuidv7()), papel felix_app já existente
-- (deploy/postgres-init.sql ou harness de testes). Não exige CREATEROLE.
-- Dinheiro em centavos (bigint); taxas em pontos-base (100 = 1%).

CREATE EXTENSION IF NOT EXISTS citext;

-- ===================== Tabelas globais (sem RLS) =====================

CREATE TABLE usuarios (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    email               citext NOT NULL UNIQUE
                        CHECK (length(email) BETWEEN 3 AND 254 AND position('@' IN email) > 1),
    nome                text NOT NULL CHECK (length(btrim(nome)) BETWEEN 1 AND 120),
    senha_hash          text NOT NULL,
    email_verificado_em timestamptz,
    criado_em           timestamptz NOT NULL DEFAULT now(),
    atualizado_em       timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessoes (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    usuario_id    uuid NOT NULL REFERENCES usuarios (id) ON DELETE CASCADE,
    token_hash    bytea NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    criado_em     timestamptz NOT NULL DEFAULT now(),
    ultimo_uso_em timestamptz NOT NULL DEFAULT now(),
    expira_em     timestamptz NOT NULL,
    ip            inet,
    user_agent    text CHECK (length(user_agent) <= 512),
    revogada_em   timestamptz
);
CREATE INDEX sessoes_usuario_ativas_idx ON sessoes (usuario_id) WHERE revogada_em IS NULL;

CREATE TABLE tokens_usuario (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    usuario_id uuid REFERENCES usuarios (id) ON DELETE CASCADE,
    finalidade text NOT NULL CHECK (finalidade IN ('redefinir_senha')),
    token_hash bytea NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    criado_em  timestamptz NOT NULL DEFAULT now(),
    expira_em  timestamptz NOT NULL,
    usado_em   timestamptz
);
CREATE INDEX tokens_usuario_usuario_idx ON tokens_usuario (usuario_id);

CREATE TABLE organizacoes (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    nome        text NOT NULL CHECK (length(btrim(nome)) BETWEEN 1 AND 120),
    criado_em   timestamptz NOT NULL DEFAULT now(),
    excluida_em timestamptz
);

CREATE TABLE membros (
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    usuario_id     uuid NOT NULL REFERENCES usuarios (id) ON DELETE CASCADE,
    papel          text NOT NULL CHECK (papel IN ('dono', 'admin', 'operador', 'leitor')),
    criado_em      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (organizacao_id, usuario_id)
);
CREATE INDEX membros_usuario_idx ON membros (usuario_id);

CREATE TABLE convites (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    email          citext NOT NULL CHECK (length(email) BETWEEN 3 AND 254 AND position('@' IN email) > 1),
    papel          text NOT NULL CHECK (papel IN ('dono', 'admin', 'operador', 'leitor')),
    token_hash     bytea NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    convidado_por  uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    criado_em      timestamptz NOT NULL DEFAULT now(),
    expira_em      timestamptz NOT NULL,
    aceito_em      timestamptz,
    revogado_em    timestamptz
);
CREATE INDEX convites_organizacao_idx ON convites (organizacao_id);
CREATE INDEX convites_email_idx ON convites (email);

CREATE TABLE assinaturas (
    organizacao_id uuid PRIMARY KEY REFERENCES organizacoes (id) ON DELETE CASCADE,
    plano          text NOT NULL CHECK (plano IN ('gratuito', 'profissional', 'empresarial')),
    status         text NOT NULL CHECK (status IN ('teste', 'ativa', 'inadimplente', 'cancelada')),
    teste_ate      timestamptz,
    provedor       text NOT NULL DEFAULT 'manual',
    criado_em      timestamptz NOT NULL DEFAULT now(),
    atualizado_em  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE jobs (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    tipo           text NOT NULL,
    organizacao_id uuid REFERENCES organizacoes (id) ON DELETE CASCADE,
    payload        jsonb NOT NULL DEFAULT '{}'::jsonb,
    status         text NOT NULL DEFAULT 'pendente'
                   CHECK (status IN ('pendente', 'executando', 'concluido', 'falhou')),
    tentativas     integer NOT NULL DEFAULT 0 CHECK (tentativas >= 0),
    executar_em    timestamptz NOT NULL DEFAULT now(),
    erro           text,
    criado_em      timestamptz NOT NULL DEFAULT now(),
    atualizado_em  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_fila_idx ON jobs (status, executar_em);
CREATE INDEX jobs_organizacao_idx ON jobs (organizacao_id, tipo, status);

CREATE TABLE emails_saida (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    para       citext NOT NULL,
    assunto    text NOT NULL,
    corpo      text NOT NULL,
    status     text NOT NULL DEFAULT 'registrado' CHECK (status IN ('registrado', 'enviado', 'falhou')),
    tentativas integer NOT NULL DEFAULT 0 CHECK (tentativas >= 0),
    erro       text,
    criado_em  timestamptz NOT NULL DEFAULT now(),
    enviado_em timestamptz
);
CREATE INDEX emails_saida_status_idx ON emails_saida (status, criado_em);

-- ===================== Tabelas de tenant (RLS forçado) =====================
-- UNIQUE (organizacao_id, id) permite FKs compostas que impedem referência
-- entre organizações diferentes.

CREATE TABLE empresas (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    razao_social   text NOT NULL CHECK (length(btrim(razao_social)) BETWEEN 1 AND 200),
    cnpj           char(14) NOT NULL CHECK (cnpj ~ '^[0-9]{14}$'),
    criado_em      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (organizacao_id, cnpj),
    UNIQUE (organizacao_id, id)
);

CREATE TABLE empenhos (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id       uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    empresa_id           uuid NOT NULL,
    ug                   char(6) NOT NULL CHECK (ug ~ '^[0-9]{6}$'),
    ug_nome              text,
    orgao                text,
    gestao               text,
    numero               text NOT NULL CHECK (length(btrim(numero)) BETWEEN 1 AND 40),
    modalidade           text,
    numero_edital_ata    text,
    valor_total_centavos bigint NOT NULL CHECK (valor_total_centavos > 0),
    data_emissao         date,
    regra_vencimento     text NOT NULL DEFAULT 'corridos_emissao'
                         CHECK (regra_vencimento IN ('corridos_emissao', 'in77_uteis')),
    prazo_dias           integer NOT NULL DEFAULT 30 CHECK (prazo_dias BETWEEN 0 AND 3650),
    origem               text NOT NULL DEFAULT 'manual'
                         CHECK (origem IN ('pdf', 'manual', 'transparencia', 'legado')),
    status               text NOT NULL DEFAULT 'ativo' CHECK (status IN ('ativo', 'anulado')),
    observacao           text,
    anulado_em           timestamptz,
    motivo_anulacao      text,
    criado_por           uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    criado_em            timestamptz NOT NULL DEFAULT now(),
    atualizado_em        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (organizacao_id, ug, numero),
    UNIQUE (organizacao_id, id),
    FOREIGN KEY (organizacao_id, empresa_id) REFERENCES empresas (organizacao_id, id)
);
CREATE INDEX empenhos_empresa_idx ON empenhos (organizacao_id, empresa_id);
CREATE INDEX empenhos_status_idx ON empenhos (organizacao_id, status, criado_em DESC);

CREATE TABLE notas (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id       uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    empenho_id           uuid NOT NULL,
    numero               text NOT NULL CHECK (length(btrim(numero)) BETWEEN 1 AND 40),
    tipo                 text NOT NULL CHECK (tipo IN ('produto', 'servico')),
    valor_centavos       bigint NOT NULL CHECK (valor_centavos > 0),
    data_emissao         date NOT NULL,
    data_recebimento     date,
    vencimento           date NOT NULL,
    status               text NOT NULL DEFAULT 'emitida'
                         CHECK (status IN ('emitida', 'liquidada', 'paga', 'cancelada')),
    valor_pago_centavos  bigint NOT NULL DEFAULT 0 CHECK (valor_pago_centavos >= 0),
    data_liquidacao      date,
    data_pagamento       date,
    documento_liquidacao text,
    documento_pagamento  text,
    criado_por           uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    criado_em            timestamptz NOT NULL DEFAULT now(),
    atualizado_em        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (empenho_id, tipo, numero),
    UNIQUE (organizacao_id, id),
    FOREIGN KEY (organizacao_id, empenho_id) REFERENCES empenhos (organizacao_id, id)
);
CREATE INDEX notas_situacao_idx ON notas (organizacao_id, status, vencimento);
CREATE INDEX notas_empenho_idx ON notas (organizacao_id, empenho_id);

CREATE TABLE eventos_nota (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    nota_id        uuid NOT NULL,
    tipo           text NOT NULL CHECK (tipo IN ('liquidada', 'paga', 'cancelada', 'reaberta')),
    data           date NOT NULL,
    valor_centavos bigint CHECK (valor_centavos >= 0),
    documento      text,
    observacao     text,
    origem         text NOT NULL DEFAULT 'manual' CHECK (origem IN ('manual', 'transparencia')),
    criado_por     uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    criado_em      timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (organizacao_id, nota_id) REFERENCES notas (organizacao_id, id) ON DELETE CASCADE
);
CREATE INDEX eventos_nota_nota_idx ON eventos_nota (organizacao_id, nota_id, criado_em);

CREATE TABLE cobrancas (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id     uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    nota_id            uuid NOT NULL,
    multa_bps          integer NOT NULL DEFAULT 0 CHECK (multa_bps BETWEEN 0 AND 10000),
    juros_bps          integer NOT NULL DEFAULT 0 CHECK (juros_bps BETWEEN 0 AND 10000),
    data_referencia    date NOT NULL,
    principal_centavos bigint NOT NULL CHECK (principal_centavos >= 0),
    multa_centavos     bigint NOT NULL CHECK (multa_centavos >= 0),
    juros_centavos     bigint NOT NULL CHECK (juros_centavos >= 0),
    total_centavos     bigint NOT NULL CHECK (total_centavos >= 0),
    dias_atraso        integer NOT NULL CHECK (dias_atraso >= 0),
    canal              text NOT NULL CHECK (canal IN ('email', 'protocolo', 'sei', 'correios', 'outro')),
    protocolo          text,
    enviada_em         date NOT NULL,
    observacao         text,
    texto              text NOT NULL,
    criado_por         uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    criado_em          timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (organizacao_id, nota_id) REFERENCES notas (organizacao_id, id)
);
CREATE INDEX cobrancas_nota_idx ON cobrancas (organizacao_id, nota_id);
CREATE INDEX cobrancas_criado_idx ON cobrancas (organizacao_id, criado_em DESC);

CREATE TABLE notificacoes (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    usuario_id     uuid REFERENCES usuarios (id) ON DELETE CASCADE,
    tipo           text NOT NULL,
    titulo         text NOT NULL,
    corpo          text NOT NULL DEFAULT '',
    link           text,
    lida_em        timestamptz,
    criado_em      timestamptz NOT NULL DEFAULT now(),
    chave_dedup    text UNIQUE
);
CREATE INDEX notificacoes_org_idx ON notificacoes (organizacao_id, lida_em, criado_em DESC);

CREATE TABLE integracoes (
    organizacao_id          uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    tipo                    text NOT NULL CHECK (tipo IN ('transparencia')),
    ativa                   boolean NOT NULL DEFAULT false,
    ultima_sincronizacao_em timestamptz,
    ultimo_status           text,
    ultimo_erro             text,
    PRIMARY KEY (organizacao_id, tipo)
);

CREATE TABLE documentos_transparencia (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    organizacao_id  uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    empresa_id      uuid NOT NULL,
    codigo          text NOT NULL,
    codigo_resumido text,
    fase            smallint NOT NULL CHECK (fase IN (1, 2, 3)),
    data            date,
    valor_centavos  bigint,
    ug              text,
    ug_nome         text,
    orgao           text,
    especie         text,
    observacao      text,
    empenho_codigo  text,
    bruto           jsonb NOT NULL DEFAULT '{}'::jsonb,
    empenho_id      uuid,
    nota_id         uuid,
    situacao        text NOT NULL DEFAULT 'novo' CHECK (situacao IN ('novo', 'vinculado', 'ignorado')),
    importado_em    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (organizacao_id, codigo),
    FOREIGN KEY (organizacao_id, empresa_id) REFERENCES empresas (organizacao_id, id) ON DELETE CASCADE,
    FOREIGN KEY (organizacao_id, empenho_id) REFERENCES empenhos (organizacao_id, id) ON DELETE SET NULL (empenho_id),
    FOREIGN KEY (organizacao_id, nota_id) REFERENCES notas (organizacao_id, id) ON DELETE SET NULL (nota_id)
);
CREATE INDEX documentos_transparencia_situacao_idx ON documentos_transparencia (organizacao_id, situacao, fase);

CREATE TABLE auditoria (
    id             bigserial PRIMARY KEY,
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    usuario_id     uuid REFERENCES usuarios (id) ON DELETE SET NULL,
    acao           text NOT NULL,
    entidade       text NOT NULL,
    entidade_id    text,
    dados          jsonb NOT NULL DEFAULT '{}'::jsonb,
    ip             inet,
    criado_em      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX auditoria_org_idx ON auditoria (organizacao_id, criado_em DESC);
CREATE INDEX auditoria_entidade_idx ON auditoria (organizacao_id, entidade, criado_em DESC);

-- ===================== RLS =====================

DO $$
DECLARE
    tabela text;
BEGIN
    FOREACH tabela IN ARRAY ARRAY[
        'empresas', 'empenhos', 'notas', 'eventos_nota', 'cobrancas', 'notificacoes',
        'documentos_transparencia', 'auditoria', 'integracoes'
    ]
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tabela);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', tabela);
        EXECUTE format(
            'CREATE POLICY isolamento_tenant ON %I
                USING (organizacao_id = nullif(current_setting(''app.org_id'', true), '''')::uuid)
                WITH CHECK (organizacao_id = nullif(current_setting(''app.org_id'', true), '''')::uuid)',
            tabela
        );
    END LOOP;
END
$$;

-- ===================== Permissões do papel felix_app =====================

GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO felix_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO felix_app;
REVOKE ALL ON TABLE _sqlx_migrations FROM felix_app;
-- Tabelas e sequências de migrações futuras (criadas pelo mesmo dono).
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO felix_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO felix_app;
