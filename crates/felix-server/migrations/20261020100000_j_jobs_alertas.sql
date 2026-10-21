-- Agente J: jobs, alertas e integração com o Portal da Transparência.

-- No máximo um job de sincronização pendente/executando por organização
-- (POST /sincronizar responde 409 quando já existe).
CREATE UNIQUE INDEX jobs_transparencia_unico_idx ON jobs (organizacao_id)
    WHERE tipo = 'transparencia_sync' AND status IN ('pendente', 'executando');

-- No máximo um job de envio de e-mail pendente/executando no sistema.
CREATE UNIQUE INDEX jobs_envio_email_unico_idx ON jobs (tipo)
    WHERE tipo = 'envio_email' AND status IN ('pendente', 'executando');

-- Agendamento diário de alertas (busca por tipo + horário).
CREATE INDEX jobs_tipo_executar_idx ON jobs (tipo, executar_em);

-- Fila de envio: e-mails registrados, por ordem de chegada.
CREATE INDEX emails_saida_registrados_idx ON emails_saida (criado_em) WHERE status = 'registrado';

-- E-mail-resumo diário de alertas: um por usuário, por organização, por dia.
CREATE TABLE alertas_email_enviados (
    organizacao_id uuid NOT NULL REFERENCES organizacoes (id) ON DELETE CASCADE,
    usuario_id     uuid NOT NULL REFERENCES usuarios (id) ON DELETE CASCADE,
    data           date NOT NULL,
    email_id       uuid REFERENCES emails_saida (id) ON DELETE SET NULL,
    criado_em      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (organizacao_id, usuario_id, data)
);

ALTER TABLE alertas_email_enviados ENABLE ROW LEVEL SECURITY;
ALTER TABLE alertas_email_enviados FORCE ROW LEVEL SECURITY;
CREATE POLICY isolamento_tenant ON alertas_email_enviados
    USING (organizacao_id = nullif(current_setting('app.org_id', true), '')::uuid)
    WITH CHECK (organizacao_id = nullif(current_setting('app.org_id', true), '')::uuid);

-- Lista de documentos por situação/fase em ordem de data.
CREATE INDEX documentos_transparencia_lista_idx
    ON documentos_transparencia (organizacao_id, situacao, data DESC, id DESC);
