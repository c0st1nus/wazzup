CREATE DATABASE main;
CREATE SCHEMA IF NOT EXISTS "public";
CREATE TABLE "companies" (
    "id" bigint NOT NULL,
    "name" varchar NOT NULL,
    "description" varchar,
    "email" varchar NOT NULL,
    "phone" varchar,
    "database_name" varchar NOT NULL,
    "wazzup_api_key" VARCHAR NOT NULL,
    "is_active" boolean,
    "created_at" timestamp,
    "updated_at" timestamp,
    "subscription_tier" varchar,
    "max_locations" bigint,
    CONSTRAINT "pk_table_1_id" PRIMARY KEY ("id")
);