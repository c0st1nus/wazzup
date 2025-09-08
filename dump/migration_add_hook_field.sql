-- Миграция для добавления поля hook в таблицу users
-- Выполнить для каждой клиентской базы данных

ALTER TABLE "public"."users" 
ADD COLUMN "hook" varchar(500);

-- Комментарий к новому полю
COMMENT ON COLUMN "public"."users"."hook" IS 'URL для webhook-вызовов ботов. Заполняется только для пользователей с ролью bot';
