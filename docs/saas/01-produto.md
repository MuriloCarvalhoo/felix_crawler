# Produto: Felix — contas a receber do governo

## Proposta

PMEs que vendem para órgãos públicos sabem, em um painel, **quanto têm a receber, o que está atrasado, de qual órgão e o que fazer agora**, sem planilha.

## Personas

| Persona | Papel no sistema | Precisa de |
|---|---|---|
| Sócio/diretor | `dono` | Visão de caixa a receber, risco por órgão, plano e usuários |
| Financeiro/faturamento | `operador` | Cadastrar empenhos/NFs, marcar pagamentos, cobrar |
| Contador/consultor externo | `leitor` (em várias organizações) | Consultar e exportar |
| Gestor da conta | `admin` | Convidar pessoas, configurar CNPJs e integrações |

## Fluxos principais

1. **Começar:** cadastro (nome, e-mail, senha, nome da organização) → organização criada com período de teste do plano Profissional por 14 dias → primeiro CNPJ cadastrado.
2. **Empenho:** enviar PDFs SIAFI → conferir campos e avisos → salvar. Ou cadastro manual. Ou importar da integração com o Portal da Transparência.
3. **Faturar:** lançar NF/NFS-e no empenho; saldo validado no servidor; vencimento calculado pela regra do empenho.
4. **Acompanhar:** painel com a receber, vencido, aging e próximos vencimentos; alertas de NF vencendo em até 5 dias e de NF vencida.
5. **Receber:** registrar liquidação e pagamento (data, documento NS/OB, valor) manualmente ou por conciliação automática.
6. **Cobrar:** calcular encargos contratuais, gerar minuta de ofício, registrar envio (canal, protocolo, data) e manter o histórico.
7. **Equipe:** convidar por e-mail com papel; remover; trocar papel.
8. **Conta:** ver plano, limites e uso; exportar CSV; trilha de auditoria.

## Escopo do MVP SaaS (este ciclo)

| Módulo | Inclui |
|---|---|
| Identidade | Cadastro, login, logout, sessão por cookie, recuperação de senha, convites, papéis, troca de organização |
| Organização | Dados, membros, convites, CNPJs (empresas fornecedoras) |
| Empenhos | Upload/análise de PDF (parser atual), cadastro manual, lista com busca/filtros, detalhe com saldo, edição, anulação |
| Notas | Cadastro, status (emitida → liquidada → paga; cancelada), eventos com data/documento, vencimento por regra, atraso |
| Cobrança | Cálculo com multa/juros, minuta, registro de cobranças, histórico |
| Painel | Indicadores, aging 0–30/31–60/61–90/90+, por UG/órgão, próximos vencimentos |
| Alertas | Notificações in-app diárias; e-mail via caixa de saída |
| Transparência | Sincronização por CNPJ (fases 1–3), documentos importados, sugestão e confirmação de vínculo |
| Planos | Gratuito/Profissional/Empresarial com limites aplicados no servidor; sem cobrança real ainda |
| Conformidade | Auditoria, exportação CSV, exclusão da organização pelo dono |
| Legado | Importação do SQLite atual para uma organização |

## Fora do MVP

OCR, armazenamento permanente de PDFs, integração com portais estaduais/municipais, antecipação de recebíveis, emissão de NF, SSO/gov.br, MFA, app móvel, API pública para clientes, painel de superadmin. Itens que dependem de decisão estão em [05-decisoes-pendentes.md](05-decisoes-pendentes.md).

## Planos (valores provisórios, preço pendente)

| Limite | Gratuito | Profissional | Empresarial |
|---|---:|---:|---:|
| CNPJs | 1 | 5 | ilimitado |
| Empenhos ativos | 30 | ilimitado | ilimitado |
| Usuários | 2 | 10 | ilimitado |
| Integração Transparência | não | sim | sim |
| Alertas por e-mail | não | sim | sim |
| Exportação CSV | sim | sim | sim |

Período de teste: 14 dias do Profissional. Ao expirar sem pagamento, a organização passa ao Gratuito; dados acima do limite ficam somente leitura (nada é apagado).

## Métricas de sucesso

- Ativação: organização com ≥ 1 empenho e ≥ 1 NF em 7 dias.
- Valor: % de NFs vencidas com cobrança registrada; dias médios de atraso por órgão.
- Retenção: organizações ativas por semana; sincronizações bem-sucedidas.
