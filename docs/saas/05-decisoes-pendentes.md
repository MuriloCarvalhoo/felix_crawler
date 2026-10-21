# Decisões que dependem do usuário

Cada item tem um padrão já implementado, para o sistema funcionar sem a decisão. Ordem = impacto no lançamento.

| # | Decisão | Padrão implementado agora | Opções | Recomendação |
|---|---|---|---|---|
| P-01 | Chave da API do Portal da Transparência | Integração aparece “não configurada”; testada com servidor falso | Cadastrar e-mail em <https://portaldatransparencia.gov.br/api-de-dados/cadastrar-email> e definir `FELIX_TRANSPARENCIA_KEY` | Cadastrar já: é o diferencial do produto. Uma chave por instalação; limite 180 req/min |
| P-02 | Provedor de pagamento e preços | Planos e limites funcionam; troca manual pelo dono, sem cobrança | Asaas (Pix/boleto/cartão, comum em PMEs BR), Stripe (cartão, Pix via parceiro), Pagar.me, Iugu | Asaas, se o público for PME brasileira que paga por boleto/Pix |
| P-03 | E-mail transacional e domínio remetente | Caixa de saída no banco; SMTP opcional por `FELIX_SMTP_URL` | Amazon SES, Resend, Postmark, SMTP próprio | SES ou Resend com SPF/DKIM/DMARC no domínio |
| P-04 | Hospedagem, domínio, TLS e backup | Dockerfile, systemd e nginx modelo; compose só para dev | VPS atual (2 CPU/4 GiB) com Postgres local; Postgres gerenciado (Neon/Supabase/RDS) | VPS + Postgres local com `pg_dump` diário cifrado fora da VPS; migrar para gerenciado quando houver clientes pagantes |
| P-05 | Nome, marca, termos de uso, privacidade (LGPD), DPO | “Felix”; sem páginas legais | Redação jurídica própria ou modelo revisado | Revisão por advogado antes de cobrar |
| P-06 | Login gov.br/SSO e MFA | E-mail + senha | MFA TOTP; login gov.br (exige credenciamento); Google/Microsoft | MFA TOTP opcional na próxima versão |
| P-07 | Abrangência além do federal | Só Portal da Transparência federal | Estados/municípios por portal (cada um diferente) | Validar com clientes quais entes concentram receita antes de integrar |
| P-08 | Guardar PDFs e OCR | PDFs temporários, sem OCR | Armazenamento S3 compatível + OCR (Tesseract) | Guardar PDFs (anexo do empenho) quando houver storage; OCR só se aparecerem digitalizações |
| P-09 | Texto do ofício e índice de atualização | Minuta atual, taxas zero, sem índice | IPCA-E, Selic, índice do contrato | Revisão jurídica do modelo; índice por contrato |
| P-10 | Migração do SQLite legado em produção | Comando `importar-sqlite` disponível | Importar agora, importar depois, descartar | Importar após backup, na organização da empresa atual |
| P-12 | Verificação de e-mail no cadastro | Cadastro libera teste de 14 dias sem confirmar e-mail; limitado a 5 cadastros/h por IP | Exigir confirmação antes do teste; antes do primeiro convite | Exigir confirmação antes de liberar o teste Profissional |
| P-13 | Pagamento parcial | OB menor que a NF marca a nota como paga e mostra o valor pago | Manter; criar status `paga_parcial`; deixar aberta até completar | Status `paga_parcial` se clientes relatarem retenções frequentes |
| P-11 | Commit/PR deste trabalho | Tudo no branch `feat/saas-multitenant`, sem commit | Commit único, commits por módulo, PR | Commits por módulo + PR para revisão |
