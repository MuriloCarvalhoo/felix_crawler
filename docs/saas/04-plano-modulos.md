# Plano de execução por módulos

Execução por subagentes em ondas. Dentro de uma onda os agentes trabalham em paralelo no mesmo diretório, cada um **dono exclusivo** dos arquivos listados. Contratos: [02-arquitetura.md](02-arquitetura.md) e [03-api-v1.md](03-api-v1.md).

## Regras para todos os agentes

1. Não editar arquivos de outro agente. Se a compilação quebrar em arquivo alheio, aguardar e tentar de novo; reportar se persistir.
2. Usar `CARGO_TARGET_DIR=target/agente-<id>` para não disputar trava do cargo.
3. Rust: lints do workspace (`unwrap`/`expect`/`panic` proibidos fora de testes), erros tipados, sem `unsafe`. Identificadores e mensagens em português, como no código atual.
4. TDD: teste primeiro, depois implementação. Testes de banco usam Postgres real (`FELIX_TEST_DATABASE_URL`); sem banco, falham.
5. Nada de commit, push ou deploy. Git: `/usr/bin/git` só para leitura (`status`, `diff`).
6. Ao terminar: `cargo fmt`, `cargo clippy --all-targets -- -D warnings` e testes do próprio módulo verdes; relatório curto com arquivos alterados, testes executados e pendências.

## Onda 0 — Fundação (4 agentes em paralelo)

| Agente | Módulo | Arquivos (dono) | Aceite |
|---|---|---|---|
| A | **M1 Fundação backend** | `Cargo.toml` raiz, `crates/felix-server/{Cargo.toml,src/main.rs,src/lib.rs,src/config.rs,src/app.rs,src/http/**,src/db/**,src/modulos/mod.rs,src/modulos/*/mod.rs (stubs),src/jobs/mod.rs (stub),migrations/**,tests/comum/**}`, `deploy/compose.dev.yml`, `deploy/postgres-init.sql`, `Makefile`, `.env.example`, `Dockerfile` | Compose sobe Postgres em 55433; migrações criam **todo** o esquema de 02-arquitetura; RLS forçado testado (sem `app.org_id` → 0 linhas; org A não vê B); envelope de erro; extratores `Sessao`, `OrgCtx{org_id, papel}` com `exigir(Papel)`; `Tx::tenant`; `/health`, `/ready`; serve `frontend/dist` com fallback SPA; harness de testes cria banco temporário por teste e helpers `criar_usuario`, `criar_org`, `login_cookie`; pipeline PDF atual preservado; remove `web/` antigo e rotas antigas |
| B | **M-core Prazos** | `crates/felix-core/src/{prazos.rs,feriados.rs}`, `lib.rs` (só `pub mod`) | Feriados nacionais fixos + Páscoa/Carnaval/Sexta-feira Santa/Corpus Christi; `dias_uteis_apos`; `vencimento(regra, emissao, recebimento, prazo_dias, valor_empenho)` com IN 77 (metade abaixo de R$ 65.492,11, arredondado para cima); testes com datas de 2026 |
| C | **M8a Cliente Transparência** | `crates/felix-transparencia/**`, membro no `Cargo.toml` raiz (uma linha) | Cliente `documentos_por_favorecido(cnpj, fase, ano)` com paginação, cabeçalho `chave-api-dados`, limitador 90/min, timeouts, erros tipados (401/429/5xx); DTO tolerante a campos ausentes; parse de `valor` `"1.234,56"`; função pura `sugerir_vinculo(doc, empenhos)` por UG + número; testes com servidor falso (`wiremock`) e fixture JSON |
| D | **F0 Fundação frontend** | `frontend/**` (cria placeholders das rotas que H, I e K assumem depois) | Vite + React 19 + TS estrito + TanStack Router (arquivos) + Query + Table + Form + Zod + Tailwind v4; proxy `/api` → 8090; cliente HTTP com envelope, `X-Felix: 1`, `credentials: 'include'`, erro tipado; formatadores BRL/data/CNPJ/UG; componentes base acessíveis (Button, Input, Select, Dialog, Toast, Table, Badge, EmptyState, Pagination, PageHeader); layout com barra lateral, seletor de organização, menu do usuário; rotas públicas: entrar, cadastrar, esqueci, redefinir, convite; guarda de autenticação via `/auth/me`; Vitest com testes dos utilitários e do cliente |

## Onda 1 — Núcleo (4 agentes em paralelo, após a Onda 0)

| Agente | Módulo | Arquivos (dono) | Aceite |
|---|---|---|---|
| E | **M2 Identidade + M3 Organização + M10 Planos + M11 Auditoria** | `src/modulos/{auth,orgs,empresas,assinatura,auditoria,dev}/**`, `src/email.rs`, `tests/{auth,orgs,empresas,assinatura}.rs` | Todos os endpoints “Público”, “Conta”, “Organização”, “Empresas” da API; Argon2id; limites de tentativa; convites com e-mail na caixa de saída; papéis da ADR-008; limites de plano 402; auditoria em todas as mutações; testes de isolamento entre orgs |
| G | **M4 Empenhos + M5 Notas + M6 Cobrança + M7 Painel + Exportação** | `src/modulos/{empenhos,notas,cobrancas,painel,exportar}/**`, `tests/{empenhos,notas,cobrancas,painel}.rs` | Endpoints correspondentes; saldo com `FOR UPDATE`; máquina de estados; vencimento via `felix-core::prazos`; ofício com texto atual adaptado; painel/aging; CSV; testes de concorrência de saldo, transições inválidas e isolamento |
| H | **F1 Conta e organização** | `frontend/src/routes/o/$orgId/configuracoes/**`, `frontend/src/routes/conta.tsx`, `frontend/src/features/{conta,orgs,equipe,plano,auditoria}/**` | Telas: perfil e senha; organização (nome, excluir); CNPJs (CRUD); equipe (membros, papéis, convites); plano/uso; auditoria paginada; criar/trocar organização |
| I | **F2 Operação** | `frontend/src/routes/o/$orgId/{index.tsx,empenhos/**,notas/**,cobrancas/**}`, `frontend/src/features/{empenhos,notas,cobrancas,painel}/**` | Painel com indicadores/aging/por UG/listas; empenhos: upload múltiplo com conferência editável, cadastro manual, lista com busca/filtros/paginação, detalhe com saldo, edição, anulação; notas: cadastro, lista com filtros de situação, detalhe com linha do tempo, eventos (liquidar/pagar/cancelar/reabrir); cobrança: cálculo ao vivo, download da minuta, registro e histórico; exportação CSV |

## Onda 2 — Automação e integrações (2 agentes) + revisão

| Agente | Módulo | Arquivos (dono) | Aceite |
|---|---|---|---|
| J | **M8b Integração + M9 Alertas + Jobs + E-mail + Legado** | `src/jobs/**`, `src/modulos/{integracoes,notificacoes}/**`, `src/legado.rs`, `tests/{jobs,integracoes,notificacoes,legado}.rs` | Worker `SKIP LOCKED`; agendador diário; alertas `vence_5d`/`vencida` com deduplicação; envio SMTP opcional; endpoints de integração (sincronizar, documentos, vincular, importar, ignorar) usando `felix-transparencia` contra servidor falso; comando `felix-server importar-sqlite <arquivo> --org <uuid>` |
| K | **F3 Notificações + Integração + acabamento** | `frontend/src/routes/o/$orgId/{integracoes,notificacoes}/**`, `frontend/src/features/{integracoes,notificacoes}/**` | Sino com contador; lista de notificações; página da integração (status, sincronizar, documentos, vincular/importar/ignorar); estados vazios e de erro |

Revisão em paralelo após J/K: `rust-reviewer`, `security-reviewer`, `react-reviewer`/`typescript-reviewer`. Correções por agente dono.

## Onda 3 — Verificação no navegador (orquestrador)

Servidor release + Postgres do compose + `frontend/dist`. Roteiro em [06-verificacao-navegador.md](06-verificacao-navegador.md), cobrindo todas as funcionalidades:

1. Cadastro → organização criada em teste Profissional → logout → login → erro de senha.
2. Esqueci a senha → e-mail na caixa de saída dev → redefinir → login com nova senha.
3. CNPJ: criar (inválido recusado, duplicado recusado), editar, excluir.
4. Upload dos 7 PDFs reais → conferência → salvar → duplicado recusado → manual.
5. Lista de empenhos: busca, filtros, paginação; detalhe; edição; anulação.
6. NF: criar, saldo excedido recusado, editar, liquidar, pagar, cancelar, reabrir; vencimento nas duas regras.
7. Cobrança: cálculo, minuta, registro, histórico, download do texto guardado.
8. Painel: valores conferidos com os dados criados; aging; por UG.
9. Alertas: gerar → sino → marcar lida.
10. Equipe: convidar operador e leitor → aceitar em outro contexto → leitor sem botões de edição e API 403 → trocar papel → remover.
11. Isolamento: segunda organização não vê dados da primeira (UI e API direta).
12. Plano: limite do Gratuito → 402 com mensagem; troca de plano.
13. Integração: sem chave → “não configurada”; com servidor falso → sincronizar → vincular/importar.
14. Auditoria, exportação CSV, troca de organização, exclusão de organização.
15. Console sem erros; leitura de tela (rótulos/landmarks) e layout em 390 px.
