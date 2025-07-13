-- Add up migration script here

-- 管理情報
CREATE TABLE IF NOT EXISTS settings(
    id BIGSERIAL PRIMARY KEY,
    note JSON NOT NULL
);

-- 瞬時電力
CREATE TABLE IF NOT EXISTS instant_epower(
    id BIGSERIAL PRIMARY KEY,
    location VARCHAR(255),
    recorded_at TIMESTAMPTZ NOT NULL,
    watt NUMERIC NOT NULL
);

-- 瞬時電流
CREATE TABLE IF NOT EXISTS instant_current(
    id BIGSERIAL PRIMARY KEY,
    location VARCHAR(255),
    recorded_at TIMESTAMPTZ NOT NULL,
    r NUMERIC NOT NULL,
    t NUMERIC
);

-- 積算電力量
CREATE TABLE IF NOT EXISTS cumlative_amount_epower(
    id BIGSERIAL PRIMARY KEY,
    location VARCHAR(255),
    recorded_at TIMESTAMPTZ NOT NULL,
    kwh NUMERIC NOT NULL
);

