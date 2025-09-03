CREATE SCHEMA IF NOT EXISTS "public";

CREATE TABLE "public"."bookings" (
    "id" BIGSERIAL,
    "code" varchar(6) NOT NULL UNIQUE,
    "service_id" bigint NOT NULL,
    "client_id" bigint NOT NULL,
    "start_datetime" timestamp with time zone NOT NULL,
    "end_datetime" timestamp with time zone NOT NULL,
    "status" varchar(255) NOT NULL DEFAULT 'confirmed',
    "notes" text,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_bookings_id" PRIMARY KEY ("id")
);
-- Indexes
CREATE INDEX "bookings_idx_bookings_service_id" ON "public"."bookings" ("service_id");
CREATE INDEX "bookings_idx_bookings_client_id" ON "public"."bookings" ("client_id");
CREATE INDEX "bookings_idx_bookings_start_datetime" ON "public"."bookings" ("start_datetime");

CREATE TABLE "public"."client_tag_assignments" (
    "tag_id" bigint NOT NULL,
    "client_id" bigint NOT NULL,
    CONSTRAINT "pk_table_24_id" PRIMARY KEY ("tag_id", "client_id")
);

CREATE TABLE "public"."wazzup_settings" (
    "wazzup_user_id" varchar NOT NULL,
    "wazzup_channel_id" varchar NOT NULL,
    "role" varchar NOT NULL,
    "receives_messages" boolean NOT NULL,
    PRIMARY KEY ("wazzup_user_id", "wazzup_channel_id")
);

CREATE TABLE "public"."task_statuses" (
    "id" bigint NOT NULL,
    "value" text,
    CONSTRAINT "pk_table_21_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."service_required_roles" (
    "id" BIGSERIAL,
    "service_id" bigint NOT NULL,
    "role_id" bigint NOT NULL,
    "quantity" integer NOT NULL,
    CONSTRAINT "pk_service_required_roles_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."task_assignments" (
    "task_id" bigint NOT NULL,
    "user_id" bigint NOT NULL,
    -- Ed said some assignnments are read-only
    "can_edit" boolean NOT NULL,
    CONSTRAINT "pk_task_assignments_task_id_user_id" PRIMARY KEY ("task_id", "user_id")
);
COMMENT ON COLUMN "public"."task_assignments"."can_edit" IS 'Ed said some assignnments are read-only';

CREATE TABLE "public"."tasks" (
    "id" BIGSERIAL,
    "name" text NOT NULL,
    "project_id" bigint NOT NULL,
    "parent_task_id" bigint,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    "content" json,
    "status_id" bigint NOT NULL,
    "previous_task_id" bigint,
    "route" varchar,
    CONSTRAINT "pk_tasks_id" PRIMARY KEY ("id")
);
-- Indexes
CREATE INDEX "tasks_idx_tasks_project_id" ON "public"."tasks" ("project_id");
CREATE INDEX "tasks_idx_tasks_parent_task_id" ON "public"."tasks" ("parent_task_id");

CREATE TABLE "public"."users" (
    "id" BIGSERIAL,
    "name" varchar(255) NOT NULL,
    "login" varchar(45) NOT NULL UNIQUE,
    "email" varchar(255) NOT NULL UNIQUE,
    "password_hash" varchar(255) NOT NULL,
    "salt" varchar(255) NOT NULL,
    "role" varchar(255) NOT NULL DEFAULT 'manager',
    "resource_id" bigint,
    "location_id" bigint,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_users_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."schedule_templates" (
    "id" BIGSERIAL,
    "resource_id" bigint NOT NULL,
    "day_of_week" smallint NOT NULL,
    "start_time" time NOT NULL,
    "end_time" time NOT NULL,
    CONSTRAINT "pk_schedule_templates_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."resources" (
    "id" BIGSERIAL,
    "name" varchar(255) NOT NULL,
    "type" varchar(255) NOT NULL,
    "role_id" bigint,
    "quantity" integer NOT NULL,
    "image_path" varchar,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_resources_id" PRIMARY KEY ("id")
);
-- Indexes
CREATE INDEX "resources_idx_resources_role_id" ON "public"."resources" ("role_id");

CREATE TABLE "public"."projects" (
    "id" BIGSERIAL,
    "name" varchar(255) NOT NULL,
    "client_id" bigint NOT NULL,
    CONSTRAINT "pk_projects_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."booking_resources" (
    "booking_id" bigint NOT NULL,
    "resource_id" bigint NOT NULL,
    "quantity_used" integer NOT NULL,
    CONSTRAINT "pk_booking_resources_booking_id_resource_id" PRIMARY KEY ("booking_id", "resource_id")
);

CREATE TABLE "public"."availability_exceptions" (
    "id" BIGSERIAL,
    "resource_id" bigint NOT NULL,
    "start_datetime" timestamp with time zone NOT NULL,
    "end_datetime" timestamp with time zone NOT NULL,
    "type" varchar(255) NOT NULL,
    "reason" text,
    CONSTRAINT "pk_availability_exceptions_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."wazzup_chats" (
    "id" varchar NOT NULL,
    "channel_id" varchar NOT NULL,
    CONSTRAINT "pk_wazzup_chats_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."wazzup_channels" (
    "id" varchar NOT NULL,
    "type" varchar NOT NULL,
    CONSTRAINT "pk_wazzup_channels_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."wazzup_messages" (
    "id" varchar NOT NULL,
    "type" varchar NOT NULL,
    "content" text NOT NULL,
    "chat_id" varchar NOT NULL,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_wazzup_messages_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."services" (
    "id" BIGSERIAL,
    "name" varchar(255) NOT NULL,
    "duration" integer NOT NULL,
    "price" integer NOT NULL,
    "description" text,
    "image_path" varchar,
    "is_active" boolean NOT NULL DEFAULT TRUE,
    "created_at" timestamp NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_services_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."client_tags" (
    "id" bigint NOT NULL,
    "value" text NOT NULL UNIQUE,
    CONSTRAINT "pk_table_23_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."tokens" (
    "id" BIGSERIAL,
    "token_hash" varchar(255) NOT NULL UNIQUE,
    "user_id" bigint NOT NULL,
    "name" varchar(255) NOT NULL,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    "last_used_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    "expires_at" timestamp with time zone NOT NULL,
    CONSTRAINT "pk_tokens_id" PRIMARY KEY ("id")
);
-- Indexes
CREATE INDEX "tokens_idx_tokens_user_id" ON "public"."tokens" ("user_id");

CREATE TABLE "public"."locations" (
    "id" BIGSERIAL,
    "name" varchar NOT NULL,
    "address" varchar NOT NULL,
    "phone" varchar NOT NULL,
    "resource_id" bigint NOT NULL,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_table_20_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."wazzup_transfers" (
    "id" BIGSERIAL,
    "chat_id" varchar NOT NULL,
    "from_user_id" bigint NOT NULL,
    "to_user_id" bigint NOT NULL,
    "message_id" varchar,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_wazzup_transfers_id" PRIMARY KEY ("id")
);
-- Indexes
CREATE INDEX "wazzup_transfers_idx_chat_id" ON "public"."wazzup_transfers" ("chat_id");
CREATE INDEX "wazzup_transfers_idx_created_at" ON "public"."wazzup_transfers" ("created_at");

CREATE TABLE "public"."resource_roles" (
    "id" BIGSERIAL,
    "name" varchar(255) NOT NULL UNIQUE,
    "description" text,
    CONSTRAINT "pk_resource_roles_id" PRIMARY KEY ("id")
);

CREATE TABLE "public"."clients" (
    "id" BIGSERIAL,
    "full_name" varchar(255) NOT NULL,
    "email" varchar(255) NOT NULL UNIQUE,
    "phone" varchar(50),
    "wazzup_chat" varchar,
    "responsible_user_id" bigint,
    "created_at" timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT "pk_clients_id" PRIMARY KEY ("id")
);

-- Foreign key constraints
-- Schema: public
ALTER TABLE "public"."client_tag_assignments" ADD CONSTRAINT "fk_client_tag_assignments_client_id_clients_id" FOREIGN KEY("client_id") REFERENCES "public"."clients"("id");
ALTER TABLE "public"."client_tag_assignments" ADD CONSTRAINT "fk_client_tag_assignments_tag_id_client_tags_id" FOREIGN KEY("tag_id") REFERENCES "public"."client_tags"("id");
ALTER TABLE "public"."clients" ADD CONSTRAINT "fk_clients_wazzup_chat_wazzup_chats_id" FOREIGN KEY("wazzup_chat") REFERENCES "public"."wazzup_chats"("id");
ALTER TABLE "public"."clients" ADD CONSTRAINT "fk_clients_responsible_user_id_users_id" FOREIGN KEY("responsible_user_id") REFERENCES "public"."users"("id");
ALTER TABLE "public"."wazzup_transfers" ADD CONSTRAINT "fk_wazzup_transfers_chat_id_wazzup_chats_id" FOREIGN KEY("chat_id") REFERENCES "public"."wazzup_chats"("id");
ALTER TABLE "public"."wazzup_transfers" ADD CONSTRAINT "fk_wazzup_transfers_from_user_id_users_id" FOREIGN KEY("from_user_id") REFERENCES "public"."users"("id");
ALTER TABLE "public"."wazzup_transfers" ADD CONSTRAINT "fk_wazzup_transfers_to_user_id_users_id" FOREIGN KEY("to_user_id") REFERENCES "public"."users"("id");
ALTER TABLE "public"."availability_exceptions" ADD CONSTRAINT "fk_availability_exceptions_resource_id_resources_id" FOREIGN KEY("resource_id") REFERENCES "public"."resources"("id");
ALTER TABLE "public"."booking_resources" ADD CONSTRAINT "fk_booking_resources_booking_id_bookings_id" FOREIGN KEY("booking_id") REFERENCES "public"."bookings"("id");
ALTER TABLE "public"."booking_resources" ADD CONSTRAINT "fk_booking_resources_resource_id_resources_id" FOREIGN KEY("resource_id") REFERENCES "public"."resources"("id");
ALTER TABLE "public"."bookings" ADD CONSTRAINT "fk_bookings_client_id_clients_id" FOREIGN KEY("client_id") REFERENCES "public"."clients"("id");
ALTER TABLE "public"."bookings" ADD CONSTRAINT "fk_bookings_service_id_services_id" FOREIGN KEY("service_id") REFERENCES "public"."services"("id");
ALTER TABLE "public"."projects" ADD CONSTRAINT "fk_projects_client_id_clients_id" FOREIGN KEY("client_id") REFERENCES "public"."clients"("id");
ALTER TABLE "public"."resources" ADD CONSTRAINT "fk_resources_role_id_resource_roles_id" FOREIGN KEY("role_id") REFERENCES "public"."resource_roles"("id");
ALTER TABLE "public"."schedule_templates" ADD CONSTRAINT "fk_schedule_templates_resource_id_resources_id" FOREIGN KEY("resource_id") REFERENCES "public"."resources"("id");
ALTER TABLE "public"."service_required_roles" ADD CONSTRAINT "fk_service_required_roles_role_id_resource_roles_id" FOREIGN KEY("role_id") REFERENCES "public"."resource_roles"("id");
ALTER TABLE "public"."service_required_roles" ADD CONSTRAINT "fk_service_required_roles_service_id_services_id" FOREIGN KEY("service_id") REFERENCES "public"."services"("id");
ALTER TABLE "public"."task_assignments" ADD CONSTRAINT "fk_task_assignments_task_id_tasks_id" FOREIGN KEY("task_id") REFERENCES "public"."tasks"("id");
ALTER TABLE "public"."task_assignments" ADD CONSTRAINT "fk_task_assignments_user_id_users_id" FOREIGN KEY("user_id") REFERENCES "public"."users"("id");
ALTER TABLE "public"."tasks" ADD CONSTRAINT "fk_tasks_parent_task_id_tasks_id" FOREIGN KEY("parent_task_id") REFERENCES "public"."tasks"("id");
ALTER TABLE "public"."tasks" ADD CONSTRAINT "fk_tasks_previous_task_id_tasks_id" FOREIGN KEY("previous_task_id") REFERENCES "public"."tasks"("id");
ALTER TABLE "public"."tasks" ADD CONSTRAINT "fk_tasks_project_id_projects_id" FOREIGN KEY("project_id") REFERENCES "public"."projects"("id");
ALTER TABLE "public"."tokens" ADD CONSTRAINT "fk_tokens_user_id_users_id" FOREIGN KEY("user_id") REFERENCES "public"."users"("id");
ALTER TABLE "public"."wazzup_chats" ADD CONSTRAINT "fk_wazzup_chats_channel_id_wazzup_channels_id" FOREIGN KEY("channel_id") REFERENCES "public"."wazzup_channels"("id");
ALTER TABLE "public"."wazzup_messages" ADD CONSTRAINT "fk_wazzup_messages_chat_id_wazzup_chats_id" FOREIGN KEY("chat_id") REFERENCES "public"."wazzup_chats"("id");
ALTER TABLE "public"."wazzup_settings" ADD CONSTRAINT "fk_wazzup_settings_wazzup_channel_id_wazzup_channels_id" FOREIGN KEY("wazzup_channel_id") REFERENCES "public"."wazzup_channels"("id");
ALTER TABLE "public"."users" ADD CONSTRAINT "fk_users_location_id_locations_id" FOREIGN KEY("location_id") REFERENCES "public"."locations"("id");
ALTER TABLE "public"."locations" ADD CONSTRAINT "fk_locations_resource_id_resources_id" FOREIGN KEY("resource_id") REFERENCES "public"."resources"("id");
ALTER TABLE "public"."tasks" ADD CONSTRAINT "fk_tasks_status_id_task_statuses_id" FOREIGN KEY("status_id") REFERENCES "public"."task_statuses"("id");
ALTER TABLE "public"."users" ADD CONSTRAINT "fk_users_resource_id_resources_id" FOREIGN KEY("resource_id") REFERENCES "public"."resources"("id");
