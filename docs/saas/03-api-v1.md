# Contrato da API v1

Contrato compartilhado entre backend e frontend. Mudanças aqui exigem atualizar os dois lados.

## Convenções

- Base: `/api/v1`. JSON UTF-8. Datas `YYYY-MM-DD`; instantes ISO-8601 UTC. Dinheiro em centavos (`*_centavos`, inteiro). Taxas em pontos-base (`*_bps`, `100` = 1%). IDs UUID.
- Envelope (ADR-009): `{"dados": X}` ou `{"dados": [..], "meta": {"total": n, "pagina": p, "por_pagina": k}}`. Erro: `{"erro": {"codigo", "mensagem", "campos"?}}`.
- Autenticação: cookie `felix_sessao`. Mutações (POST/PUT/PATCH/DELETE) exigem cabeçalho `X-Felix: 1` e `Origin` igual ao host, quando presente.
- `{org}` = UUID da organização. O usuário precisa ser membro. Registro de outra organização → 404.
- Papel mínimo indicado entre colchetes: [leitor] [operador] [admin] [dono].

## Público

| Método | Caminho | Corpo / Query | Resposta |
|---|---|---|---|
| GET | `/health` | — | `{"status":"ok"}` (fora de `/api/v1`) |
| GET | `/ready` | — | 200 se banco responde, 503 caso contrário |
| POST | `/auth/cadastro` | `{nome, email, senha, organizacao_nome}` | 201 `Me`, define cookie |
| POST | `/auth/login` | `{email, senha}` | 200 `Me`, define cookie; 401 `credenciais_invalidas` genérico |
| POST | `/auth/logout` | — | 204, apaga cookie |
| POST | `/auth/senha/esqueci` | `{email}` | 204 sempre |
| POST | `/auth/senha/redefinir` | `{token, senha}` | 204; revoga sessões |
| GET | `/convites/{token}` | — | `{organizacao_nome, email, papel, usuario_existe}` |
| POST | `/convites/{token}/aceitar` | `{nome?, senha?}` (obrigatórios se usuário não existe) | 200 `Me`, define cookie se criou usuário |
| GET | `/dev/emails` | só `FELIX_ENV=dev` | `[{id, para, assunto, corpo, status, criado_em}]` |

`Me` = `{"usuario": {id, nome, email}, "organizacoes": [{id, nome, papel, plano}]}`.

## Conta

| Método | Caminho | Corpo | Resposta |
|---|---|---|---|
| GET | `/auth/me` | — | `Me` (401 sem sessão) |
| PATCH | `/auth/me` | `{nome}` | `Me` |
| POST | `/auth/me/senha` | `{senha_atual, senha_nova}` | 204; revoga outras sessões |
| POST | `/orgs` | `{nome}` | 201 `{id, nome, papel:"dono", plano}` |

## Organização

| Método | Caminho | Papel | Corpo / Query | Resposta |
|---|---|---|---|---|
| GET | `/orgs/{org}` | leitor | — | `{id, nome, papel, assinatura: Assinatura}` |
| PATCH | `/orgs/{org}` | admin | `{nome}` | igual ao GET |
| DELETE | `/orgs/{org}` | dono | `{confirmacao: nome da org}` | 204 (exclusão lógica) |
| GET | `/orgs/{org}/membros` | leitor | — | `[{usuario_id, nome, email, papel, desde}]` |
| PATCH | `/orgs/{org}/membros/{usuario_id}` | admin | `{papel}` | membro; não rebaixa o último dono; só dono promove a dono |
| DELETE | `/orgs/{org}/membros/{usuario_id}` | admin | — | 204; não remove o último dono |
| GET | `/orgs/{org}/convites` | admin | — | `[{id, email, papel, expira_em, criado_em}]` pendentes |
| POST | `/orgs/{org}/convites` | admin | `{email, papel}` | 201 convite; e-mail com link `/convite/{token}`; 402 se exceder usuários |
| DELETE | `/orgs/{org}/convites/{id}` | admin | — | 204 |
| GET | `/orgs/{org}/assinatura` | leitor | — | `Assinatura` |
| POST | `/orgs/{org}/assinatura` | dono | `{plano}` | `Assinatura` (provedor manual: troca imediata) |
| GET | `/orgs/{org}/auditoria` | admin | `pagina, por_pagina, entidade?` | lista `[{id, usuario_nome, acao, entidade, entidade_id, dados, criado_em}]` |

`Assinatura` = `{plano, status, teste_ate, limites: {cnpjs, empenhos_ativos, usuarios, transparencia, alertas_email}, uso: {cnpjs, empenhos_ativos, usuarios}}`; limite ilimitado = `null`.

## Empresas (CNPJs)

| Método | Caminho | Papel | Corpo | Resposta |
|---|---|---|---|---|
| GET | `/orgs/{org}/empresas` | leitor | — | `[Empresa]` |
| POST | `/orgs/{org}/empresas` | admin | `{razao_social, cnpj}` | 201 `Empresa`; 409 duplicado; 402 limite |
| PATCH | `/orgs/{org}/empresas/{id}` | admin | `{razao_social}` | `Empresa` |
| DELETE | `/orgs/{org}/empresas/{id}` | admin | — | 204; 409 se houver empenhos |

`Empresa` = `{id, razao_social, cnpj, criado_em}`. CNPJ aceito com ou sem máscara; armazenado com 14 dígitos; dígitos verificadores validados.

## Empenhos

| Método | Caminho | Papel | Corpo / Query | Resposta |
|---|---|---|---|---|
| POST | `/orgs/{org}/empenhos/analisar` | operador | multipart `arquivos` (até 10 PDFs) | `{"dados": [{arquivo, analise?: {perfil, dados, avisos}, erro?, empresa_id_sugerida?, ja_cadastrado: bool}]}` |
| GET | `/orgs/{org}/empenhos` | leitor | `busca?, empresa_id?, ug?, status?=ativo, com_saldo?, pagina, por_pagina` | lista `[EmpenhoResumo]` |
| POST | `/orgs/{org}/empenhos` | operador | `NovoEmpenho` | 201 `Empenho`; 409 duplicado UG+número; 402 limite |
| GET | `/orgs/{org}/empenhos/{id}` | leitor | — | `Empenho` com `notas: [Nota]` |
| PATCH | `/orgs/{org}/empenhos/{id}` | operador | campos de `NovoEmpenho` exceto `empresa_id`; `valor_total_centavos` ≥ faturado | `Empenho`; recalcula vencimento das notas abertas se regra/prazo mudar |
| POST | `/orgs/{org}/empenhos/{id}/anular` | operador | `{motivo}` | `Empenho`; 409 se houver nota paga |

`NovoEmpenho` = `{empresa_id, ug, ug_nome?, orgao?, gestao?, numero, modalidade?, numero_edital_ata?, valor_total_centavos, data_emissao?, regra_vencimento?, prazo_dias?, origem?("pdf"|"manual"), observacao?}`.
`EmpenhoResumo` = `{id, empresa_id, empresa_cnpj, ug, ug_nome, orgao, numero, modalidade, valor_total_centavos, faturado_centavos, recebido_centavos, saldo_centavos, status, origem, data_emissao, criado_em}`.
`Empenho` = `EmpenhoResumo` + `{gestao, numero_edital_ata, regra_vencimento, prazo_dias, observacao, notas?}`.

## Notas fiscais

| Método | Caminho | Papel | Corpo / Query | Resposta |
|---|---|---|---|---|
| GET | `/orgs/{org}/notas` | leitor | `status?` (vírgulas), `empenho_id?`, `situacao?` (`vencida`,`a_vencer`,`vence_7d`), `busca?`, `pagina, por_pagina`, `ordenar?=vencimento` | lista `[Nota]` |
| POST | `/orgs/{org}/empenhos/{id}/notas` | operador | `{numero, tipo, valor_centavos, data_emissao, data_recebimento?}` | 201 `Nota`; 409 saldo/duplicada |
| GET | `/orgs/{org}/notas/{id}` | leitor | — | `Nota` com `eventos: [Evento]`, `cobrancas: [CobrancaResumo]` |
| PATCH | `/orgs/{org}/notas/{id}` | operador | `{numero?, valor_centavos?, data_emissao?, data_recebimento?}` só se `emitida` | `Nota` |
| POST | `/orgs/{org}/notas/{id}/eventos` | operador (`reaberta`: admin) | `{tipo, data, valor_centavos?, documento?, observacao?}` | `Nota`; 409 transição inválida |

`Nota` = `{id, empenho_id, empenho_numero, ug, ug_nome, empresa_cnpj, numero, tipo, valor_centavos, data_emissao, data_recebimento, vencimento, status, dias_atraso, valor_pago_centavos, data_liquidacao, data_pagamento, documento_liquidacao, documento_pagamento, criado_em}`.
`Evento` = `{id, tipo, data, valor_centavos, documento, observacao, origem, criado_por_nome, criado_em}`.
Pagamento: `valor_centavos` padrão = valor da nota; pagamento menor que o valor fica `paga` com diferença visível (`valor_pago_centavos`).

## Cobranças

| Método | Caminho | Papel | Corpo / Query | Resposta |
|---|---|---|---|---|
| GET | `/orgs/{org}/notas/{id}/calculo` | leitor | `multa_bps=0, juros_bps=0, data_referencia?=hoje` | `{vencimento, dias_atraso, principal_centavos, multa_centavos, juros_centavos, total_centavos}` |
| GET | `/orgs/{org}/notas/{id}/oficio` | leitor | `multa_bps, juros_bps` | `text/plain` anexo `Oficio_<numero>.txt` (minuta, não registra) |
| POST | `/orgs/{org}/notas/{id}/cobrancas` | operador | `{multa_bps, juros_bps, canal, protocolo?, enviada_em, observacao?}` | 201 `Cobranca` (guarda cálculo e texto) |
| GET | `/orgs/{org}/cobrancas` | leitor | `nota_id?, pagina, por_pagina` | lista `[CobrancaResumo]` |
| GET | `/orgs/{org}/cobrancas/{id}/oficio` | leitor | — | `text/plain` do texto guardado |

`CobrancaResumo` = `{id, nota_id, nota_numero, empenho_numero, ug, canal, protocolo, enviada_em, total_centavos, dias_atraso, criado_por_nome, criado_em}`; `Cobranca` = resumo + `{multa_bps, juros_bps, principal_centavos, multa_centavos, juros_centavos, observacao, texto}`.

## Painel

`GET /orgs/{org}/painel` [leitor] →
```json
{"dados": {
  "a_receber_centavos": 0, "vencido_centavos": 0, "vence_30d_centavos": 0, "recebido_mes_centavos": 0,
  "notas_abertas": 0, "notas_vencidas": 0, "empenhos_ativos": 0, "saldo_empenhos_centavos": 0,
  "aging": [{"faixa": "a_vencer|1_30|31_60|61_90|90_mais", "centavos": 0, "quantidade": 0}],
  "por_ug": [{"ug": "160203", "ug_nome": "…", "aberto_centavos": 0, "vencido_centavos": 0, "atraso_medio_dias": 0}],
  "proximos_vencimentos": [Nota], "mais_atrasadas": [Nota]
}}
```

## Notificações

| Método | Caminho | Papel | Resposta |
|---|---|---|---|
| GET | `/orgs/{org}/notificacoes?nao_lidas=true` | leitor | lista `[{id, tipo, titulo, corpo, link, lida_em, criado_em}]`, `meta.nao_lidas` |
| POST | `/orgs/{org}/notificacoes/{id}/lida` | leitor | 204 |
| POST | `/orgs/{org}/notificacoes/lidas` | leitor | 204 (todas) |
| POST | `/orgs/{org}/alertas/gerar` | admin | 200 `{criadas}` — roda o job de alertas agora (útil em teste) |

## Integração Portal da Transparência

| Método | Caminho | Papel | Corpo / Query | Resposta |
|---|---|---|---|---|
| GET | `/orgs/{org}/integracoes/transparencia` | leitor | — | `{configurada_no_servidor, permitida_no_plano, ativa, ultima_sincronizacao_em, ultimo_status, ultimo_erro, pendentes}` |
| PATCH | `/orgs/{org}/integracoes/transparencia` | admin | `{ativa}` | igual |
| POST | `/orgs/{org}/integracoes/transparencia/sincronizar` | admin | `{ano?}` | 202 `{job_id}`; 409 se já executando; 402 plano; 503 sem chave |
| GET | `/orgs/{org}/integracoes/transparencia/documentos` | leitor | `situacao?=novo, fase?, pagina` | lista `[{id, empresa_cnpj, codigo, codigo_resumido, fase, data, valor_centavos, ug, ug_nome, orgao, observacao, empenho_codigo, situacao, sugestao: {tipo:"empenho"|"nota", id, rotulo}?}]` |
| POST | `/orgs/{org}/integracoes/transparencia/documentos/{id}/vincular` | operador | `{empenho_id?}` ou `{nota_id?}` | documento; vínculo de fase 2/3 a nota cria evento `liquidada`/`paga` com origem `transparencia` |
| POST | `/orgs/{org}/integracoes/transparencia/documentos/{id}/importar` | operador | — (fase 1) | 201 `Empenho` com origem `transparencia` |
| POST | `/orgs/{org}/integracoes/transparencia/documentos/{id}/ignorar` | operador | — | documento |

## Exportação

| Método | Caminho | Papel | Resposta |
|---|---|---|---|
| GET | `/orgs/{org}/exportar/empenhos.csv` | leitor | CSV `;`, UTF-8 com BOM, valores `1234,56` |
| GET | `/orgs/{org}/exportar/notas.csv` | leitor | idem |
