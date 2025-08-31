INSERT INTO "companies" (
    "id", 
    "name", 
    "email", 
    "phone", 
    "database_name", 
    "wazzup_api_key",
    "is_active", 
    "created_at", 
    "updated_at", 
    "subscription_tier", 
    "max_locations"
) VALUES 
( 
  22289228,
 'AiTomaton', 
 'info@aitomaton.kz', 
 '+77477255072', 
 'client_22289228', 
 '316c7f283893478aaf7ba4bb9e8c3eb4', 
 true, 
 '2024-01-15 10:30:00+00', 
 '2024-08-20 14:22:00+00', 
 'premium', 
 5
) ON CONFLICT (id) DO NOTHING;