# Guia interno do backend (para quem implementa módulos)

Fundação entregue na Onda 0. Ler junto com [02-arquitetura.md](02-arquitetura.md) e [03-api-v1.md](03-api-v1.md).

## Ambiente

- Banco: `docker compose -f deploy/compose.dev.yml up -d` → `felix-db` em `127.0.0.1:55433` (superusuário `postgres/postgres`; app `felix/felix`, banco `felix`).
- Testes: `FELIX_TEST_DATABASE_URL` (padrão `postgres://postgres:postgres@127.0.0.1:55433/postgres`). Cada `TestApp` cria e apaga um banco `felix_t_*`. Sem banco, os testes falham.
- Use `CARGO_TARGET_DIR=target/agente-<id>`.

## Peças disponíveis (`felix_server::*`)

- `app::AppState { pool, config, pipeline, admissao }` (Clone). `estado.hoje()` = data BRT (use para vencimento/atraso, passe como parâmetro SQL); `estado.agora()`. Relógio fixável em teste.
- `config::Config { ambiente: Ambiente::{Dev,Prod}, bind, database_url, public_url, frontend_dir, transparencia_key: Option, transparencia_url, smtp_url: Option, email_from: Option }`, `cookie_seguro()`, `producao()`.
- `http::ErroApi`: `nao_autenticado()`, `credenciais_invalidas()`, `proibido(msg)`, `papel_insuficiente()`, `nao_encontrado()`, `nao_encontrado_msg(msg)`, `validacao(msg)`, `validacao_campo(campo,msg)`, `validacao_campos(BTreeMap)->Option<Self>`, `.com_campo(c,m)`, `conflito(msg)`, `limite_plano(msg)`, `ocupado()`, `indisponivel(msg)`, `interno()`, `interno_log(ctx, e)`. `From<sqlx::Error>`: RowNotFound→404, unique→409, FK→409, check→422.
- `http::resposta`: `dados(x)`, `criado(x)` (201), `sem_conteudo()` (204), `lista(itens,total,pagina,por_pagina)` + `.com_meta(chave, valor)`; `Paginacao` (`pagina()`, `por_pagina()`, `limite()`, `offset()`, `lista(itens,total)`), usável com `#[serde(flatten)]`.
- `http::entrada`: **usar** `Corpo<T>` (JSON), `Consulta<T>` (query), `Caminho<T>` (path) — erros já no envelope.
- `http::sessao`: `Sessao { sessao_id, usuario_id }`, `SessaoOpcional`, `criar_sessao(executor, cookie_seguro, usuario_id, ip, user_agent) -> NovaSessao { sessao_id, token, cookie: HeaderValue }`, `cookie_remocao(seguro)`, `revogar_sessao(pool,id)`, `revogar_sessoes_do_usuario(&mut conn, usuario_id, manter)`, `ClienteInfo { ip, user_agent }`.
- `http::org`: `Papel::{Leitor<Operador<Admin<Dono}` (serde minúsculo, sqlx texto), `OrgCtx { org_id, usuario_id, sessao_id, papel }` lendo `{org}` do caminho (401/404), `.exigir(Papel)` (403).
- `db::tenant(&pool, org_id) -> Transaction` (já com `SET LOCAL ROLE felix_app` + `app.org_id`); `db::auditar(&mut *tx, org_id, Some(usuario), acao, entidade, Some(id), json, ip)`.
- `seguranca`: `hash_senha_async`, `verificar_senha_async`, `validar_senha_forte`, `gerar_token`, `token_bem_formado`, `hash_token`.
- `pdf::receber_e_analisar(&estado, multipart) -> Vec<ResultadoAnalise { arquivo, analise, erro }>`; rota precisa `.layer(DefaultBodyLimit::max(pdf::LIMITE_CORPO_ANALISE))`.
- `modulos::<nome>::rotas() -> Router<AppState>` com caminhos completos relativos a `/api/v1`. Rotas duplicadas entre módulos derrubam o axum.

## Esquema: detalhes que importam

- FKs compostas `(organizacao_id, x_id)`; excluir empresa com empenhos → 409.
- Colunas extras: `empenhos.anulado_em`, `empenhos.motivo_anulacao`, `emails_saida.tentativas`, `emails_saida.erro`, `usuarios.atualizado_em`, `assinaturas.criado_em`.
- `notificacoes.chave_dedup` é única global: inclua o id da organização na chave.
- Migrações novas: arquivo novo em `crates/felix-server/migrations/` com timestamp maior; privilégios para `felix_app` são automáticos.

## Harness (`tests/comum/mod.rs`)

`TestApp::novo()`, `com(Opcoes{..})`, `fixar_hoje(data)`, `criar_usuario`, `criar_org` (profissional em teste), `adicionar_membro`, `cookie_de`, `membro(org, Papel) -> (id, cookie)`, `dono_com_org() -> (usuario, org, cookie)`, `tenant(org)`, `requisicao(...)`, `multipart(...)`, builder `req(Method, caminho).cookie(..).json(..).sem_x_felix().enviar() -> Resposta { status, headers, corpo, json }`, `encerrar()`. Arquivos de teste: `#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]`.
