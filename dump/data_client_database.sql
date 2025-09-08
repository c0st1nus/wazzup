-- Начальные данные для клиентской базы данных
-- Пользователи системы

-- Вставляем пользователей
INSERT INTO "public"."users" ("id", "name", "login", "email", "password_hash", "salt", "role", "resource_id", "location_id", "hook") VALUES
(1, 'Системный администратор', 'admin', 'admin@example.com', 
 'c7ad44cbad762a5da0a452f9e854fdc1e0e7a52a38015f23f3eab1d80b931dd472634dfac71cd34ebc35d16ab7fb8a90c81f975113d6c7538dc69dd8de9077ec', 
 'admin_salt_2025', 'admin', NULL, NULL, NULL),
(2, 'Менеджер Анна Петрова', 'manager', 'manager@example.com',
 'ef92b778bafe771e89245b89ecbc08a44a4e166c06659911881f383d4473e94f3b946b4f85f6a4cb2e9ed3dc2e3b4b3e59bedc0b3f7e9af5b8c1e3d5b47c28a2c',
 'manager_salt_2025', 'manager', NULL, NULL, NULL),
(3, 'Сотрудник ОКК Иван Сидоров', 'quality_control', 'qc@example.com',
 'a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3c7d8b54e5f8b3e6e8f4d5b2a9c3e7b8a5d6f9e2b4c8a7d3f5e9b1c6a8d4e7f2b5c9',
 'qc_salt_2025', 'quality_control', NULL, NULL, NULL),
(4, 'Бот-помощник', 'bot', 'bot@system.local',
 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855b5d0d6e6cf5fcb8b42e8a8e7b6f5a9c3d8e2f7b4a6c9d5e8f1b3a7e4d9c2f6b8a5',
 'bot_salt_2025', 'bot', NULL, NULL, 'https://example.com/bot/webhook')
ON CONFLICT ("id") DO NOTHING;
