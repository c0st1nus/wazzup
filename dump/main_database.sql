CREATE SCHEMA IF NOT EXISTS "public";
CREATE TABLE IF NOT EXISTS "companies" (
    "id" bigint NOT NULL,
    "name" varchar NOT NULL,
    "description" varchar,
    "email" varchar NOT NULL,
    "phone" varchar,
    "database_name" varchar NOT NULL,
    "wazzup_api_key" VARCHAR NOT NULL,
    "is_active" boolean,
    "created_at" timestamptz,
    "updated_at" timestamptz,
    "subscription_tier" varchar,
    "max_locations" bigint,
    CONSTRAINT "pk_table_1_id" PRIMARY KEY ("id")
);

-- Migration to add created_at column to wazzup_messages table
-- Run this on all client databases

ALTER TABLE wazzup_messages 
ADD COLUMN IF NOT EXISTS created_at timestamptz NOT NULL DEFAULT NOW();

-- Create an index on created_at for performance
CREATE INDEX IF NOT EXISTS idx_wazzup_messages_created_at 
ON wazzup_messages(created_at);

-- Create a composite index for duplicate detection
CREATE INDEX IF NOT EXISTS idx_wazzup_messages_duplicate_check 
ON wazzup_messages(chat_id, type, content);