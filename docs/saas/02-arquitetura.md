# Arquitetura

## Visão geral

```text
Navegador (SPA TanStack)  ──HTTPS, cookie felix_sessao──▶  felix-server (Axum)
                                                             │  /api/v1/*  JSON
                                                             │  /*         frontend/dist (SPA)
                                                             ├─ Pipeline PDF (Poppler isolado, 2 workers)
                                                             ├─ Worker de jobs (Tokio, SKIP LOCKED)
                                                             │    ├─ transparencia_sync ──▶ api.portaldatransparencia.gov.br
                                                             │    ├─ alertas_diarios
                                                             │    └─ envio_email ──▶ SMTP (opcional)
                                                             └─ sqlx ──▶ PostgreSQL 18 (RLS forçado)
```

## Workspace

```text
crates/felix-core/          domínio puro: dinheiro, cobrança, prazos (dias úteis), parser SIAFI
crates/felix-transparencia/ cliente da API da CGU + conciliação (sem banco)
crates/felix-server/        config, http (axum), auth, módulos, jobs, migrações, CLI
  migrations/               SQL numerado (sqlx)
  src/http/                 erro/envelope, extratores (Sessao, OrgCtx), middleware
  src/modulos/<nome>/       mod.rs (rotas) + repo.rs (SQL) + tipos.rs
frontend/                   Vite + React + TanStack (Router, Query, Table, Form) + Tailwind v4
deploy/                     compose de dev, systemd, nginx (modelos)
docs/saas/                  pesquisa, produto, decisões, API, plano, verificação
```

## Configuração (variáveis)

| Variável | Padrão | Uso |
|---|---|---|
| `FELIX_ENV` | `dev` | `dev` ou `prod`; `prod` exige cookie Secure e desativa rotas `/dev` |
| `FELIX_BIND` | `127.0.0.1:8090` | Endereço HTTP |
| `DATABASE_URL` | `postgres://felix:felix@127.0.0.1:55433/felix` | Conexão (usuário deve ser membro de `felix_app`) |
| `FELIX_PUBLIC_URL` | `http://127.0.0.1:8090` | Links em e-mails; origem aceita |
| `FELIX_FRONTEND_DIR` | `frontend/dist` | Arquivos estáticos da SPA |
| `FELIX_TRANSPARENCIA_KEY` | vazio | Chave da CGU; vazio desativa a integração |
| `FELIX_TRANSPARENCIA_URL` | `https://api.portaldatransparencia.gov.br` | Base (testes usam servidor falso) |
| `FELIX_SMTP_URL`, `FELIX_EMAIL_FROM` | vazio | Envio real de e-mail |
| `FELIX_TEST_DATABASE_URL` | — | Banco administrativo para testes (cria bancos temporários) |
| `RUST_LOG` | `felix_server=info` | Logs JSON |

## Modelo de dados (PostgreSQL)

Tabelas globais (sem RLS): `usuarios`, `sessoes`, `tokens_usuario` (redefinição/convite), `organizacoes`, `membros`, `convites`, `assinaturas`, `jobs`, `emails_saida`.
Tabelas de tenant (com `organizacao_id` e RLS forçado): `empresas`, `empenhos`, `notas`, `eventos_nota`, `cobrancas`, `notificacoes`, `documentos_transparencia`, `auditoria`, `integracoes`.

> `membros`, `convites` e `assinaturas` têm `organizacao_id`, mas são consultadas antes de existir contexto de organização (login, aceite de convite). Elas não usam RLS; o acesso é sempre filtrado por código e coberto por testes.

```sql
usuarios(id uuid pk, email citext unique, nome text, senha_hash text, email_verificado_em timestamptz, criado_em timestamptz)
sessoes(id uuid pk, usuario_id fk, token_hash bytea unique, criado_em, ultimo_uso_em, expira_em, ip inet, user_agent text, revogada_em)
tokens_usuario(id uuid pk, usuario_id fk null, finalidade text check in('redefinir_senha'), token_hash bytea unique, expira_em, usado_em)
organizacoes(id uuid pk, nome text, criado_em, excluida_em null)
membros(organizacao_id fk, usuario_id fk, papel text check in('dono','admin','operador','leitor'), criado_em, pk(organizacao_id, usuario_id))
convites(id uuid pk, organizacao_id fk, email citext, papel, token_hash bytea unique, convidado_por fk, expira_em, aceito_em, revogado_em)
assinaturas(organizacao_id pk fk, plano text check in('gratuito','profissional','empresarial'), status text check in('teste','ativa','inadimplente','cancelada'), teste_ate timestamptz, provedor text default 'manual', atualizado_em)

empresas(id uuid pk, organizacao_id, razao_social, cnpj char(14), criado_em, unique(organizacao_id, cnpj))
empenhos(id uuid pk, organizacao_id, empresa_id fk, ug char(6), ug_nome text, orgao text, gestao text null, numero text,
         modalidade text, numero_edital_ata text, valor_total_centavos bigint check > 0, data_emissao date null,
         regra_vencimento text check in('corridos_emissao','in77_uteis') default 'corridos_emissao', prazo_dias int default 30,
         origem text check in('pdf','manual','transparencia','legado'), status text check in('ativo','anulado'),
         observacao text, criado_por, criado_em, atualizado_em, unique(organizacao_id, ug, numero))
notas(id uuid pk, organizacao_id, empenho_id fk, numero text, tipo text check in('produto','servico'), valor_centavos bigint > 0,
      data_emissao date, data_recebimento date null, vencimento date, status text check in('emitida','liquidada','paga','cancelada'),
      valor_pago_centavos bigint default 0, data_liquidacao date, data_pagamento date, documento_liquidacao text, documento_pagamento text,
      criado_por, criado_em, atualizado_em, unique(empenho_id, tipo, numero))
eventos_nota(id uuid pk, organizacao_id, nota_id fk, tipo text check in('liquidada','paga','cancelada','reaberta'), data date,
             valor_centavos bigint null, documento text null, observacao text null, origem text check in('manual','transparencia'), criado_por null, criado_em)
cobrancas(id uuid pk, organizacao_id, nota_id fk, multa_bps int, juros_bps int, data_referencia date, principal_centavos, multa_centavos,
          juros_centavos, total_centavos, dias_atraso int, canal text check in('email','protocolo','sei','correios','outro'),
          protocolo text null, enviada_em date, observacao text null, texto text, criado_por, criado_em)
notificacoes(id uuid pk, organizacao_id, usuario_id null, tipo text, titulo text, corpo text, link text, lida_em null, criado_em, chave_dedup text unique)
integracoes(organizacao_id, tipo text, ativa bool, ultima_sincronizacao_em, ultimo_status text, ultimo_erro text, pk(organizacao_id, tipo))
documentos_transparencia(id uuid pk, organizacao_id, empresa_id, codigo text, codigo_resumido text, fase smallint, data date, valor_centavos bigint,
          ug text, ug_nome text, orgao text, especie text, observacao text, empenho_codigo text null, bruto jsonb,
          empenho_id null, nota_id null, situacao text check in('novo','vinculado','ignorado'), importado_em, unique(organizacao_id, codigo))
auditoria(id bigserial pk, organizacao_id, usuario_id null, acao text, entidade text, entidade_id text, dados jsonb, ip inet null, criado_em)
jobs(id uuid pk, tipo text, organizacao_id null, payload jsonb, status text check in('pendente','executando','concluido','falhou'),
     tentativas int, executar_em timestamptz, erro text, criado_em, atualizado_em)
emails_saida(id uuid pk, para citext, assunto text, corpo text, status text check in('registrado','enviado','falhou'), criado_em, enviado_em)
```

Saldo do empenho = `valor_total_centavos − soma(valor das notas não canceladas)`. A gravação de NF trava a linha do empenho (`SELECT … FOR UPDATE`) para impedir saldo negativo concorrente.

### Máquina de estados da nota

```text
emitida ──liquidada──▶ liquidada ──paga──▶ paga
   │                       │                 │
   └──cancelada──▶ cancelada ◀──cancelada────┘(não: paga não cancela)
liquidada/paga/cancelada ──reaberta──▶ emitida  (somente admin/dono; registra evento)
emitida ──paga──▶ paga (liquidação implícita na mesma data)
```

Vencimento: calculado na criação/edição da NF pela regra do empenho, guardado em `notas.vencimento` para consultas de aging. Dias de atraso = `max(0, hoje_BRT − vencimento)` enquanto não paga/cancelada.

## Segurança

- Sessão e CSRF: ADR-006. Cabeçalhos: CSP `default-src 'self'`, `frame-ancestors 'none'`, `nosniff`, `no-referrer`, `no-store` em `/api`.
- Erros 500 não expõem detalhes; log com `request_id`.
- Corpo JSON até 64 KiB; upload até 20 MiB com limites atuais do pipeline.
- Senhas: 10–128 caracteres, rejeita as 100 mais comuns.
- RLS: ADR-004. Toda query de tenant passa por `Tx::tenant(pool, org_id)`.
