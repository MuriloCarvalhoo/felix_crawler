# Registro de decisões (ADR)

Formato curto: contexto → decisão → alternativas → consequência. Data de todas: 20/10/2026 (BRT). Status “Tomada” = decidida sem depender do usuário e reversível. Itens que dependem do usuário estão em [05-decisoes-pendentes.md](05-decisoes-pendentes.md).

## ADR-001 Backend continua em Rust (Axum + Tokio) — Tomada
- **Contexto:** requisito do usuário; já existe parser SIAFI, pipeline Poppler e cálculo testados.
- **Decisão:** manter workspace Rust 2024, `felix-core` (domínio puro) e `felix-server` (HTTP). Novo crate `felix-transparencia` (cliente e conciliação, sem dependência de banco).
- **Consequência:** lints atuais mantidos (`unsafe` proibido; `unwrap`/`expect`/`panic` negados).

## ADR-002 Frontend em TanStack Router (SPA) + Query + Table + Form, sem TanStack Start — Tomada
- **Contexto:** usuário permitiu TanStack. TanStack Start traria runtime Node/SSR em produção, duplicando backend.
- **Decisão:** SPA Vite + React + TypeScript em `frontend/`, roteamento por arquivos (`@tanstack/router-plugin`), dados por TanStack Query, tabelas por TanStack Table, formulários por TanStack Form + Zod. Tailwind CSS v4. Build estático servido pelo binário Rust na mesma origem.
- **Alternativas:** TanStack Start (SSR, sem necessidade de SEO no app logado); manter HTML/JS puro (não escala).
- **Consequência:** produção continua um binário + Postgres; Node só para build e testes.

## ADR-003 PostgreSQL 18 com sqlx, substituindo SQLite — Tomada
- **Contexto:** multi-tenant, jobs concorrentes, RLS e backup online. SQLite não tem RLS.
- **Decisão:** Postgres 18, `sqlx` com consultas em tempo de execução (`query_as`, sem macros que exigem banco na compilação), migrações em `crates/felix-server/migrations/` aplicadas no start (`sqlx::migrate!`). Desenvolvimento: `docker compose` com `postgres:18-alpine` em `127.0.0.1:55433`.
- **Consequência:** SQLite fica só como origem do importador legado (`felix-server importar-sqlite`).

## ADR-004 Multi-tenancy: esquema compartilhado + `organizacao_id` + RLS forçado — Tomada
- **Decisão:** toda tabela de tenant tem `organizacao_id uuid not null`. RLS `ENABLE` + `FORCE`, política `USING` e `WITH CHECK` em `organizacao_id = current_setting('app.org_id', true)::uuid`. Toda operação de tenant abre transação, executa `SET LOCAL ROLE felix_app` e `set_config('app.org_id', $1, true)`. `felix_app` é papel sem login e sem `BYPASSRLS`. A aplicação também filtra por `organizacao_id` (defesa em profundidade).
- **Alternativas:** schema por tenant, banco por tenant (custo operacional alto para PMEs).
- **Consequência:** teste obrigatório prova que consulta sem `app.org_id` não retorna linhas e que org A não lê org B.

## ADR-005 Identificadores UUID v7 — Tomada
- Ordenáveis por tempo, não enumeráveis em URL. Valores monetários continuam `bigint` em centavos; taxas em pontos-base (`100` = 1%).

## ADR-006 Autenticação própria com sessão opaca em cookie — Tomada
- **Decisão:** senha com Argon2id (parâmetros OWASP: m=19 MiB, t=2, p=1). Sessão: token aleatório de 32 bytes no cookie `felix_sessao` (`HttpOnly`, `SameSite=Lax`, `Secure` fora de dev, `Path=/`); banco guarda só SHA-256 do token. Expiração deslizante de 7 dias, absoluta de 30 dias. Logout revoga. Troca/redefinição de senha revoga as outras sessões.
- **CSRF:** `SameSite=Lax` + verificação de `Origin`/`Host` nas mutações + cabeçalho obrigatório `X-Felix: 1` em métodos não-GET.
- **Limite de tentativas:** login e “esqueci a senha” limitados por IP e por e-mail em memória (janela de 15 min).
- **Alternativas:** JWT (revogação ruim), IdP externo (decisão pendente P-06).

## ADR-007 Roteamento por organização no caminho — Tomada
- `/api/v1/orgs/{org_id}/...`. O servidor confere a associação usuário↔organização a cada requisição e aplica o papel. Permite abas com organizações diferentes.

## ADR-008 Papéis e permissões — Tomada
| Ação | dono | admin | operador | leitor |
|---|:-:|:-:|:-:|:-:|
| Ler dados da organização | ✓ | ✓ | ✓ | ✓ |
| Criar/editar empenhos, NFs, cobranças, eventos | ✓ | ✓ | ✓ | |
| CNPJs, convites, membros, integrações | ✓ | ✓ | | |
| Auditoria | ✓ | ✓ | | |
| Plano, excluir organização, transferir dono | ✓ | | | |

## ADR-009 Envelope de resposta da API — Tomada
- Sucesso: `{"dados": ..., "meta": {...}}` (meta só em listas: `total`, `pagina`, `por_pagina`). Erro: `{"erro": {"codigo": "validacao", "mensagem": "…", "campos": {"campo": "mensagem"}}}`. Códigos: `nao_autenticado` 401, `proibido` 403, `nao_encontrado` 404, `validacao` 422, `conflito` 409, `limite_plano` 402, `ocupado` 429, `interno` 500. Registros de outra organização respondem 404, não 403.

## ADR-010 Vencimento por regra configurável — Tomada
- `corridos_emissao` (padrão, comportamento atual: emissão + N dias corridos) e `in77_uteis` (recebimento da NF + 10 dias úteis de liquidação + 10 dias úteis de pagamento; metade se valor do empenho ≤ R$ 65.492,11). Dias úteis usam feriados nacionais fixos e móveis (Páscoa) calculados em `felix-core`. Feriados locais não entram (registrado como limitação).

## ADR-011 Jobs em processo com tabela Postgres — Tomada
- Tabela `jobs` com `FOR UPDATE SKIP LOCKED`; worker Tokio no mesmo binário, concorrência 2. Agendador diário (06:00 BRT) cria alertas. Sem Redis/fila externa.

## ADR-012 E-mail por caixa de saída — Tomada (provedor pendente P-03)
- Todo e-mail vira linha em `emails_saida`. Se `FELIX_SMTP_URL` estiver configurado, o worker envia via SMTP (`lettre`, TLS); senão, só registra. Em `FELIX_ENV=dev`, `GET /api/v1/dev/emails` lista a caixa de saída para testes de convite e redefinição.

## ADR-013 Planos aplicados no servidor; cobrança real adiada — Tomada (provedor pendente P-02)
- Tabela `assinaturas` (plano, status, fim do teste). Limites verificados nas criações; excesso retorna 402 `limite_plano`. Interface `ProvedorPagamento` com implementação `manual`; webhook real fica para a decisão P-02.

## ADR-014 Integração com Portal da Transparência — Tomada (chave pendente P-01)
- Crate `felix-transparencia` com `reqwest` (rustls), URL base configurável, cabeçalho `chave-api-dados`, limite próprio de 90 req/min (metade do limite oficial de 180) e paginação até página vazia. Documentos guardados em `documentos_transparencia` (único por organização + código). Conciliação sugere vínculo por UG + número do empenho (`documentoResumido`/`documento`); confirmação é humana. Sem chave: integração aparece como “não configurada”. Testes usam servidor HTTP falso.

## ADR-015 PDFs continuam temporários — Tomada
- Mantido o pipeline atual (Poppler isolado, limites de CPU/memória, 2 workers). Nada de armazenamento permanente sem decisão P-08.

## ADR-016 Uma origem em produção, proxy em desenvolvimento — Tomada
- Produção: Rust serve `frontend/dist` (fallback para `index.html` nas rotas da SPA) e `/api`. Desenvolvimento: Vite em `127.0.0.1:5179` com proxy de `/api` para `127.0.0.1:8090`. CSP restritiva mantida.

## ADR-017 Paginação por página/tamanho — Tomada
- `?pagina=1&por_pagina=25` (máx. 100). Listas maiores que isso não são necessárias nos volumes de PMEs.

## ADR-018 Auditoria de mutações — Tomada
- `auditoria(organizacao_id, usuario_id, acao, entidade, entidade_id, dados jsonb, ip, criado_em)`. Gravada na mesma transação da mutação. Sem dados de senha/token.

## ADR-019 Testes — Tomada
- Rust: testes de integração contra Postgres real do compose (`FELIX_TEST_DATABASE_URL`); sem banco, os testes **falham** (não pulam). Cada teste cria um banco temporário. Frontend: Vitest + Testing Library; verificação ponta a ponta no navegador ao final.
