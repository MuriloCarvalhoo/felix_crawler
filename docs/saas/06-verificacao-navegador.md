# Verificação no navegador

Data: 20/10/2026, 21h35–21h50 BRT (UTC−3). Chrome (extensão Claude in Chrome) contra o binário release em `127.0.0.1:8090`, `FELIX_ENV=dev`, banco limpo `felix_verif` no Postgres do compose, `frontend/dist` de produção e um servidor falso da API da CGU em `127.0.0.1:18099` (script de teste fora do repositório, com 2 empenhos, 1 liquidação e 1 pagamento para o CNPJ 19.116.488/0001-45).

Contas criadas só nesse banco local de teste: `dono@felix.test` (dono) e `leitor@felix.test` (leitor). Não são credenciais de nenhum ambiente real.

## Resultado por funcionalidade

| # | Funcionalidade | Resultado |
|---|---|---|
| 1 | Cadastro → organização em teste Profissional; senha curta e senha comum recusadas (cliente e servidor) | ✓ |
| 2 | Login com senha errada → mensagem genérica; logout; login com senha nova | ✓ (bug corrigido, ver abaixo) |
| 3 | Esqueci a senha → e-mail na caixa de saída → redefinir → sessão antiga 401 → token reutilizado 422 | ✓ |
| 4 | CNPJ: inválido recusado, duplicado recusado, alfanumérico `12.ABC.345/01DE-35` aceito, editar, excluir | ✓ |
| 5 | Upload dos 7 PDFs reais: 7/7 extraídos com valores corretos (R$ 32.450,00 … R$ 9.875,00), empresa sugerida pelo CNPJ, "Salvar todos" | ✓ |
| 6 | Reenvio do mesmo PDF → selo "Já cadastrado" e 409 explicado | ✓ |
| 7 | Empenho manual com regra IN 77/2022; edição; anulação com motivo; anulação recusada com notas abertas (409) | ✓ |
| 8 | NF com vencimento IN 77 (valor ≤ R$ 65.492,11 → 5+5 dias úteis): 10/09 → 24/09/2026 e 15/09 → 29/09/2026, conferidos à mão | ✓ |
| 9 | Saldo excedido bloqueado no cliente e no servidor (409 com saldo disponível); POST sem `X-Felix` → 403 | ✓ |
| 10 | Lista de notas ordenada por vencimento; filtro "vencidas"; busca de empenhos | ✓ |
| 11 | Ciclo da NF: liquidar (NS) → pagar (OB) → reabrir → cancelar; linha do tempo com autor; transição inválida 409; foco volta ao título | ✓ |
| 12 | Cobrança: multa 2%, juros 1% a.m., 19 dias → R$ 20,00 + R$ 6,33 = R$ 1.026,33; minuta com arts. 92, V, e 141; registro via SEI; histórico e lista de cobranças | ✓ |
| 13 | Painel: a receber R$ 13.000,00, vencido R$ 13.000,00, saldo de empenhos R$ 183.704,21, aging (1–30: R$ 3 mil/2; 61–90: R$ 10 mil/1), por UG com atraso médio 81 e 24 dias — todos conferidos com os dados criados | ✓ |
| 14 | Equipe: convites de leitor e operador; aceite com criação de conta; leitor sem botões de edição; API 403 em mutações e auditoria | ✓ |
| 15 | Nova organização (Gratuito) pelo seletor; isolamento: dados da org A pela rota da org B → 404 | ✓ |
| 16 | Limite do plano: 2º CNPJ no Gratuito → 402 com mensagem; integração "fora do plano"; troca para Profissional | ✓ |
| 17 | Portal da Transparência (servidor falso): sincronizar → 4 documentos com sugestões → vincular empenho, liquidação e pagamento à NF (eventos com origem "transparencia") → importar empenho novo → 2ª sincronização em menos de 1 h → 409 | ✓ |
| 18 | Alertas: "Gerar alertas agora" → 2 notas vencidas → sino "2 não lidas" → marcar todas → sino zerado | ✓ |
| 19 | Auditoria com todas as ações; CSV de notas com BOM, `;`, valores `1234,56`, datas dd/mm/aaaa | ✓ |
| 20 | Exclusão de organização exigindo o nome; redireciona para a organização restante | ✓ |
| 21 | Página Minha conta; console sem erros no painel, empenho, integrações e conta | ✓ |

## Defeitos encontrados e corrigidos durante a verificação

1. **Login:** após senha errada, o campo era limpo e já exibia "Informe a senha." Agora o campo é reiniciado sem erro (`FormEntrar.tsx`, `resetField`).
2. **Integração:** texto acessível "Nota sugerido" → "Nota sugerida".
3. **Auditoria:** ações no formato `empenho.anular`, `cobranca.registrar` apareciam cruas; agora viram "Anulou", "Registrou" (com teste).
4. **`/ready`:** passou a informar `executores_jobs` (trabalhadores vivos).

## Limitações desta verificação

- Capturas de tela falharam na extensão (renderer sem resposta para `captureScreenshot`); a conferência foi feita pelo texto e pela árvore de acessibilidade da página. Layout em 390 px foi verificado pelos agentes de frontend em Chrome headless, não nesta sessão.
- Downloads de arquivo (minuta, CSV) foram conferidos pela resposta HTTP, sem salvar arquivos pelo navegador.
- A API real da CGU não foi chamada (sem chave, P-01). SMTP real não foi testado (P-03).
- Observações para decisão de produto: pagamento parcial vindo do Portal (OB menor que a NF) marca a nota como paga com `valor_pago` menor; o nome da UG não é preenchido a partir do Portal ao vincular um empenho já existente.

## Testes automatizados no fim do ciclo

- Rust: `cargo test --workspace` — 230 testes, 24 suítes, 0 falhas; `cargo clippy --workspace --all-targets -D warnings` e `cargo fmt --check` limpos.
- Frontend: `npm run lint` limpo; `npm test` — 27 arquivos, 156 testes; `npm run build` ok.
