# Pesquisa: como deve ser um SaaS de recebíveis públicos para fornecedores

Data: 20/10/2026, 19h BRT (UTC−3). Fontes consultadas listadas no fim. Nenhuma fonte é parecer jurídico.

## 1. Problema

Empresas que vendem para o governo recebem uma **nota de empenho** (reserva orçamentária), entregam, emitem NF/NFS-e e então esperam **liquidação** (atesto da entrega) e **pagamento** (ordem bancária). As três fases são controladas pelo órgão (SIAFI/SIAFIC). O fornecedor costuma acompanhar tudo em planilha:

- quanto do empenho já foi faturado (saldo);
- quando cada NF deveria ser paga e quantos dias está atrasada;
- qual órgão/UG concentra atrasos;
- quando e como cobrar (ofício, protocolo, SEI) e com quais encargos.

A pesquisa não encontrou produto brasileiro consolidado que una **controle de empenhos + NFs + vencimentos + cobrança + conciliação automática com dados públicos** do lado do fornecedor. O mercado existente se divide em:

| Segmento | Exemplos | Lado atendido | Lacuna para o fornecedor |
|---|---|---|---|
| Captação de licitações | Effecti, ConLicitação, Siga Pregão | Fornecedor (antes do contrato) | Termina na adjudicação; não acompanha recebíveis |
| Gestão de contratos pública | Comprasnet Contratos, ContratosGov | Órgão público | Visão do órgão; fornecedor só consulta |
| ERP público | Betha, Thema, Softplan, Elotech, FreeBalance | Órgão público | Não é produto para fornecedor |
| Antecipação de recebíveis | AntecipaGov (Seges/Serpro), FIDCs, Fomenti | Financiadores/fornecedor | Exige saber o saldo e a situação do empenho |

**Oportunidade:** “contas a receber do governo” para PMEs fornecedoras, com o empenho como unidade central.

## 2. Regras de negócio relevantes

1. **Fases da despesa:** empenho → liquidação → pagamento. Cada NF passa por essas fases; o produto precisa de status por NF, não só por empenho.
2. **Prazos federais (IN SEGES/ME nº 77/2022, art. 7º):** até 10 dias úteis para liquidar, contados do recebimento da NF; até 10 dias úteis para pagar após a liquidação. Prazos caem pela metade para contratações abaixo do limite do art. 75, II, da Lei 14.133/2021 (R$ 65.492,11 em 2026, Decreto nº 12.807/2025). Estados e municípios seguem norma própria e o edital/contrato. **Consequência:** o produto oferece regras de vencimento configuráveis por empenho: “dias corridos da emissão” (comportamento atual) e “IN 77/2022 (dias úteis)”.
3. **Ordem cronológica (Lei 14.133/2021, art. 141):** por fonte de recursos e categoria (bens, locações, serviços, obras). O órgão publica mensalmente a ordem e as justificativas de quebra. A preterição pode configurar crime (CP, art. 337-H). O ofício deve **pedir informação sobre a posição na fila**, sem acusar irregularidade só pelo atraso.
4. **Encargos (Lei 14.133/2021, art. 92, V):** o contrato define critérios de atualização monetária e compensação financeira. Multa e juros dependem do contrato; o padrão do produto é zero e editável.
5. **Cobrança administrativa recomendada:** pedir por escrito o motivo do atraso, o estágio da despesa, a data da liquidação e a previsão de pagamento; registrar protocolo. O produto deve guardar o histórico de cobranças.

## 3. Dados públicos aproveitáveis

**API do Portal da Transparência (CGU)**, verificada no OpenAPI oficial (`https://api.portaldatransparencia.gov.br/v3/api-docs`):

- Autenticação: cabeçalho `chave-api-dados`, chave gratuita obtida por cadastro de e-mail.
- `GET /api-de-dados/despesas/documentos-por-favorecido?codigoPessoa={CNPJ}&fase={1|2|3}&ano={AAAA}&pagina={n}`: empenhos, liquidações e pagamentos emitidos para o CNPJ. Campos: `data`, `documento`, `documentoResumido`, `fase`, `especie`, `valor`, `codigoUg`, `ug`, `orgao`, `observacao`, `numeroProcesso`…
- `GET /despesas/empenhos-impactados?codigoDocumento=…&fase=…`: liga liquidação/pagamento ao empenho.
- `GET /despesas/documentos/{codigo}` e `/despesas/itens-de-empenho`.
- Limite: `documentos-por-favorecido` é API restrita, 180 req/min; excesso suspende o token por 8 h.
- Cobertura: **somente despesa federal**. Estados/municípios exigem integração por portal.

**Impacto:** para fornecedores federais, o sistema pode **descobrir empenhos novos e marcar NFs como liquidadas/pagas automaticamente**. É o principal diferencial e justifica o plano pago.

Outras fontes avaliadas e deixadas para depois: Compras.gov.br Dados Abertos (módulo contratos; relatos de endpoints instáveis em 2026), API Comprasnet Contratos, NFS-e Nacional, AntecipaGov (só via gov.br, sem API pública).

## 4. O que um SaaS deste tipo precisa ter

| Área | Requisito mínimo | Referência |
|---|---|---|
| Multi-tenant | Organização isolada; usuário pode participar de várias | Padrão SaaS B2B |
| Isolamento | `organizacao_id` em toda tabela + Row-Level Security no Postgres, com `FORCE` e papel sem superusuário | tenaxum, guias RLS |
| Identidade | Cadastro, login, sessão, convite, papéis, recuperação de senha, auditoria | OWASP ASVS |
| Planos | Limites por plano, período de teste, cobrança recorrente (Pix/boleto/cartão no Brasil) | Mercado BR |
| Núcleo | Empenhos (PDF + manual), NFs, saldo, status por fase, vencimento, atraso | Domínio |
| Cobrança | Cálculo, minuta de ofício, histórico de envio/protocolo | Lei 14.133 |
| Painel | A receber, vencido, aging, por órgão, próximos vencimentos | Contas a receber |
| Alertas | Vencendo e vencidas; in-app e e-mail | Contas a receber |
| Integração | Portal da Transparência por CNPJ | CGU |
| LGPD | Exportação, exclusão, minimização, registro de acesso | LGPD |
| Operação | Health/ready, logs estruturados, backup, limites de recursos | Projeto atual |

## 5. Fontes

- ConLicitação — Empenho: <https://conlicitacao.com.br/empenho/>
- ConLicitação — Como cobrar a Administração: <https://conlicitacao.com.br/artigos-juridicos/tudo-que-voce-precisa-saber-para-cobrar-a-administracao-publica/>
- Effecti — Atraso de pagamento do governo: <https://effecti.com.br/atraso-pagamento-governo/>
- IN ME/SEGES nº 77/2022 (reprodução): <https://www.lex.com.br/instrucao-normativa-me-sed-sg-no-77-de-4-de-novembro-de-2022/>
- Ronny Charles — IN sobre ordem cronológica: <https://ronnycharles.com.br/?p=8693>
- Lei 14.133/2021, art. 141: <https://www.legjur.com/legislacao/art/lei_00141332021-141>
- Atricon — Ordem cronológica: <https://cdn.atricon.org.br/uploads/2022/10/AnexoUnico_Res_08-2014-Ordem-Cronologica-de-Pagamentos.pdf>
- Serpro — AntecipaGov: <https://serpro.gov.br/menu/noticias/noticias-2021/antecipagov-1>
- ConLicitação — AntecipaGov: <https://conlicitacao.com.br/antecipagov-o-que-e-e-como-funciona/>
- Portal da Transparência — OpenAPI: <https://api.portaldatransparencia.gov.br/v3/api-docs>
- Limites da API (projeto comunitário): <https://github.com/guizsantos/portaldatransparencia>
- Manual API Compras.gov.br: <https://www.gov.br/compras/pt-br/acesso-a-informacao/manuais/manual-dados-abertos/manual-api-compras.pdf>
- tenaxum (Axum + RLS): <https://docs.rs/tenaxum>
- Multi-tenant com RLS: <https://makerkit.dev/blog/md/tutorials/multi-tenant-saas-architecture>
- TanStack Start v1: <https://tanstack.com/blog/announcing-tanstack-start-v1>
