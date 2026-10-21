# felix-transparencia

Cliente da API de dados do Portal da Transparência (CGU) e conciliação de empenhos (ADR-014).

- `Cliente::novo(Config)`; `Config::nova(chave)` usa `https://api.portaldatransparencia.gov.br`,
  intervalo de 667 ms (90 req/min), timeout de 20 s e no máximo 50 páginas.
- `documentos_por_favorecido(cnpj, fase, ano)` e `empenhos_impactados(codigo_documento, fase)`
  paginam de `pagina=1` até a primeira lista vazia e devolvem `Paginado { itens, truncado,
  requisicoes }` (`truncado` = parou no máximo de páginas sem ver a página vazia).
  `documentos_por_favorecido_ate(..., paginas)` limita as requisições (orçamento).
- Respostas acima de 16 MiB são recusadas durante a leitura (o corpo é lido em partes).
- Funções puras: `numero_empenho`, `sugerir_empenho`, `sugerir_nota` (só sugerem; quem confirma é uma pessoa).

## Testes com servidor falso (wiremock)

Aponte `Config::base_url` para `MockServer::uri()` e use `intervalo_minimo: Duration::ZERO`.
O cliente envia `GET` com os cabeçalhos `chave-api-dados: <chave>` e `Accept: application/json`.

| Caminho | Query | Resposta 200 |
|---|---|---|
| `/api-de-dados/despesas/documentos-por-favorecido` | `codigoPessoa` (CNPJ só dígitos), `fase` (1, 2 ou 3), `ano`, `pagina` | lista JSON de `DespesasPorDocumentoDTO` |
| `/api-de-dados/despesas/empenhos-impactados` | `codigoDocumento`, `fase` (2 ou 3), `pagina` | lista JSON de `EmpenhoImpactadoBasicoDTO` |

Responda `[]` na última página: sem ela o cliente segue até `max_paginas`.
Para os erros, use 401/403 (`ChaveInvalida`), 429 (`LimiteExcedido`) e 5xx (`Indisponivel`).

Fixtures prontas em `tests/fixtures/` (UG `160203`, CNPJ `19116488000145`, ano 2026):

- `empenhos_pagina1.json`: `2026NE000290` (`"32.450,00"`) e `2026NE000291` (`"36.657,30"`).
- `empenhos_pagina2.json`: `2026NE000292` (`"13.351,00"`).
- `liquidacoes.json`: `2026NS000123` (`"32.450,00"`), documento `160203000012026NS000123`.
- `pagamentos.json`: `2026OB000456` (`"29.383,48"`, líquido de retenções).
- `empenhos_impactados.json`: dois subitens do empenho `160203000012026NE000290`.
- `documentos_incompletos.json`: campos ausentes, nulos e valores numéricos.

Item mínimo de `documentos-por-favorecido` (os demais campos do DTO são opcionais e ficam em `Documento::bruto`):

```json
{
  "data": "15/01/2026",
  "documento": "160203000012026NE000290",
  "documentoResumido": "2026NE000290",
  "fase": "Empenho",
  "especie": "Original",
  "codigoFavorecido": "19116488000145",
  "valor": "32.450,00",
  "codigoUg": "160203",
  "ug": "UNIDADE GESTORA 160203",
  "orgao": "Comando do Exército",
  "observacao": "...",
  "numeroProcesso": "64000.000123/2026-11"
}
```

Item de `empenhos-impactados`: `{"empenho": "160203000012026NE000290", "empenhoResumido": "2026NE000290", "subitem": "30", "valorLiquidado": "32.450,00", "valorPago": "0,00"}`.

Leitura tolerante: `data` aceita `dd/mm/aaaa` e `aaaa-mm-dd`; `valor` aceita `"1.234,56"`, `"1234.56"` ou um número JSON.
Os valores viram `valor_centavos: i64`. A `fase` de cada item vem do texto `fase`; se ele não vier, vale a fase consultada.
