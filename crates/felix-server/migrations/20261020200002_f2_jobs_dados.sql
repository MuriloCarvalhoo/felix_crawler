-- Agente F2: robustez de jobs, e-mail com espera entre tentativas, sincronização
-- com o Portal por partes e integridade de valores pagos.

-- E-mail: espera entre tentativas (1, 5 e 30 min). NULL = pode enviar já.
ALTER TABLE emails_saida ADD COLUMN proxima_tentativa_em timestamptz;
DROP INDEX IF EXISTS emails_saida_registrados_idx;
CREATE INDEX emails_saida_registrados_idx
    ON emails_saida (proxima_tentativa_em NULLS FIRST, criado_em)
    WHERE status = 'registrado';

-- Sincronização manual: no máximo uma por organização por hora.
ALTER TABLE integracoes ADD COLUMN ultima_sincronizacao_manual_em timestamptz;

-- Liquidações/pagamentos cujos empenhos impactados já foram consultados no
-- Portal (mesmo sem empenho identificado): não repetir a consulta a cada sincronização.
-- (O dono das tabelas também está sujeito ao RLS forçado; as correções de dados
-- abaixo desligam o FORCE só dentro desta migração, que roda numa transação.)
ALTER TABLE documentos_transparencia
    ADD COLUMN impactados_consultados boolean NOT NULL DEFAULT false;
ALTER TABLE documentos_transparencia NO FORCE ROW LEVEL SECURITY;
UPDATE documentos_transparencia SET impactados_consultados = true
    WHERE fase IN (2, 3) AND empenho_codigo IS NOT NULL;
ALTER TABLE documentos_transparencia FORCE ROW LEVEL SECURITY;

-- Valor pago nunca acima do valor da nota. Versões anteriores aceitavam o valor
-- bruto do documento do Portal ao vincular; esses casos são limitados ao valor
-- da nota antes de criar a restrição.
ALTER TABLE notas NO FORCE ROW LEVEL SECURITY;
UPDATE notas SET valor_pago_centavos = valor_centavos
    WHERE valor_pago_centavos > valor_centavos;
ALTER TABLE notas FORCE ROW LEVEL SECURITY;
ALTER TABLE notas ADD CONSTRAINT notas_valor_pago_ate_valor
    CHECK (valor_pago_centavos <= valor_centavos);

-- Limpeza diária de registros antigos.
CREATE INDEX IF NOT EXISTS jobs_encerrados_idx ON jobs (atualizado_em)
    WHERE status IN ('concluido', 'falhou');
CREATE INDEX IF NOT EXISTS notificacoes_lidas_idx ON notificacoes (organizacao_id, lida_em)
    WHERE lida_em IS NOT NULL;
