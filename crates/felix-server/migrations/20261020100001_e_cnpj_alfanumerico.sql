-- CNPJ alfanumérico (IN RFB 2.229/2024, em vigor desde julho/2026):
-- 12 posições [A-Z0-9] + 2 dígitos verificadores, armazenado em maiúsculas sem máscara.
ALTER TABLE empresas DROP CONSTRAINT IF EXISTS empresas_cnpj_check;
ALTER TABLE empresas
    ADD CONSTRAINT empresas_cnpj_check CHECK (cnpj ~ '^[0-9A-Z]{12}[0-9]{2}$');
