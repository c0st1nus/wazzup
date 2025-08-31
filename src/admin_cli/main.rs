use clap::{Parser, Subcommand};
use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, JsonValue, Statement, Value,
};
use serde_json::{Map, Value as JsonValueSerde};
use std::fs;
use wazzup::config::Config;

// Определяем структуру команд CLI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, verbatim_doc_comment)]
/// Утилита командной строки для администрирования Wazzup.
/// Позволяет управлять миграциями, данными и выполнять запросы к БД.
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Команды для работы с базами данных (основной и клиентскими).
    Db {
        #[command(subcommand)]
        db_command: DbCommand,
    },
}

#[derive(Subcommand, Debug)]
enum DbCommand {
    /// Применяет SQL-скрипты из папки /dump для создания схемы и заполнения начальными данными.
    Seed {
        /// Тип базы данных: 'main' или 'client'.
        #[arg(short, long)]
        db_type: String,

        /// ID компании (обязательно для 'client').
        #[arg(short, long)]
        company_id: Option<i64>,
    },
    /// ПОЛНОСТЬЮ удаляет все таблицы из указанной базы данных. Используйте с осторожностью!
    Wipe {
        /// Тип базы данных: 'main' или 'client'.
        #[arg(short, long)]
        db_type: String,

        /// ID компании (обязательно для 'client').
        #[arg(short, long)]
        company_id: Option<i64>,
    },
    /// Выполняет SELECT-запрос к указанной таблице и выводит результат в формате JSON.
    Query {
        /// Тип базы данных: 'main' или 'client'.
        #[arg(short, long)]
        db_type: String,

        /// ID компании (обязательно для 'client').
        #[arg(short, long)]
        company_id: Option<i64>,

        /// Имя таблицы для запроса.
        #[arg(short, long)]
        table: String,

        /// Условие WHERE для фильтрации (например, "id = 123" или "is_active = true").
        #[arg(short, long)]
        filter: Option<String>,
    },
    /// Обновляет одну запись в таблице по ее ID.
    Update {
        /// Тип базы данных: 'main' или 'client'.
        #[arg(short, long)]
        db_type: String,

        /// ID компании (обязательно для 'client').
        #[arg(short, long)]
        company_id: Option<i64>,

        /// Имя таблицы для обновления.
        #[arg(short, long)]
        table: String,

        /// ID записи, которую нужно обновить.
        #[arg(long)]
        id: i64,

        /// Данные для обновления в формате JSON (например, '{"name": "Новое имя", "is_active": false}').
        #[arg(short, long)]
        data: String,
    },
}

// Функция для получения соединения с БД
async fn get_db_connection(
    db_type: &str,
    company_id: Option<i64>,
    config: &Config,
) -> Result<DatabaseConnection, Box<dyn std::error::Error>> {
    let db_url = match db_type {
        "main" => {
            let main_db_url = std::env::var("DATABASE_URL")?;
            
            // Подключаемся к postgres БД для создания main БД
            let postgres_url = main_db_url.replace("/main", "/postgres");
            let postgres_db = Database::connect(&postgres_url).await?;
            
            // Пытаемся создать main БД
            let create_db_query = "CREATE DATABASE main".to_string();
            match postgres_db.execute(Statement::from_string(
                postgres_db.get_database_backend(),
                create_db_query,
            )).await {
                Ok(_) => println!("Создана основная база данных: main"),
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("already exists") || error_str.contains("42P04") {
                        println!("База данных main уже существует");
                    } else {
                        return Err(e.into());
                    }
                }
            }
            
            main_db_url
        }
        "client" => {
            let id = company_id.ok_or("Для клиентской БД необходимо указать --company-id")?;
            
            // Для клиентской БД сначала создаем базу данных, если она не существует
            let db_name = format!("client_{}", id);
            
            // Подключаемся к основной БД PostgreSQL для создания клиентской БД
            let main_db_url = std::env::var("DATABASE_URL")?;
            let postgres_url = main_db_url.replace("/main", "/postgres");
            let postgres_db = Database::connect(&postgres_url).await?;
            
            // Пытаемся создать клиентскую БД
            let create_db_query = format!("CREATE DATABASE {}", db_name);
            match postgres_db.execute(Statement::from_string(
                postgres_db.get_database_backend(),
                create_db_query,
            )).await {
                Ok(_) => println!("Создана клиентская база данных: {}", db_name),
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("already exists") || error_str.contains("42P04") {
                        println!("База данных {} уже существует", db_name);
                    } else {
                        return Err(e.into());
                    }
                }
            }
            
            config
                .client_database_url_template
                .replace("{db_name}", &db_name)
        }
        _ => return Err("Неверный тип БД. Используйте 'main' или 'client'".into()),
    };
    Ok(Database::connect(db_url).await?)
}

// Функция для выполнения SQL файла
async fn execute_sql_file(
    db: &DatabaseConnection,
    file_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Выполнение скрипта: {}", file_path);
    let sql = fs::read_to_string(file_path)?;
    // Разделяем на отдельные запросы, если в файле их несколько
    for query in sql.split(';').filter(|s| !s.trim().is_empty()) {
        let trimmed_query = query.trim();
        
        match db.execute(Statement::from_string(
            db.get_database_backend(),
            trimmed_query.to_string(),
        )).await {
            Ok(_) => {},
            Err(e) => {
                let error_str = e.to_string();
                // Игнорируем ошибки "уже существует" для баз данных и других объектов
                if error_str.contains("already exists") || 
                   error_str.contains("уже существует") ||
                   error_str.contains("42P04") { // PostgreSQL код для "database already exists"
                    println!("Объект уже существует, пропускаем: {}", 
                            trimmed_query.split_whitespace().take(3).collect::<Vec<_>>().join(" "));
                    continue;
                } else {
                    return Err(e.into());
                }
            }
        }
    }
    println!("Скрипт успешно выполнен.");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("Не удалось загрузить конфигурацию");
    let cli = Cli::parse();

    match &cli.command {
        Commands::Db { db_command } => match db_command {
            DbCommand::Seed { db_type, company_id } => {
                let db = get_db_connection(db_type, *company_id, &config).await?;
                println!("Применяем сиды для БД типа '{}'...", db_type);

                match db_type.as_str() {
                    "main" => {
                        execute_sql_file(&db, "dump/main_database.sql").await?;
                        execute_sql_file(&db, "dump/data_main_database.sql").await?;
                    }
                    "client" => {
                        execute_sql_file(&db, "dump/client_database.sql").await?;
                    }
                    _ => {}
                }
                println!("Сиды успешно применены.");
            }
            DbCommand::Wipe { db_type, company_id } => {
                let db = get_db_connection(db_type, *company_id, &config).await?;
                println!("Очистка БД типа '{}'...", db_type);

                let tables_query = match db.get_database_backend() {
                    sea_orm::DatabaseBackend::Postgres => {
                        "SELECT tablename FROM pg_tables WHERE schemaname = 'public'"
                    }
                    _ => unimplemented!("Очистка для других БД не реализована"),
                };

                let tables: Vec<String> = db
                    .query_all(Statement::from_string(
                        db.get_database_backend(),
                        tables_query.to_string(),
                    ))
                    .await?
                    .into_iter()
                    .filter_map(|row| row.try_get::<String>("", "tablename").ok())
                    .collect();

                if tables.is_empty() {
                    println!("Таблицы не найдены. База данных уже пуста.");
                } else {
                    for table in tables {
                        let drop_query = format!("DROP TABLE IF EXISTS \"{}\" CASCADE;", table);
                        db.execute(Statement::from_string(
                            db.get_database_backend(),
                            drop_query,
                        ))
                        .await?;
                        println!("Удалена таблица: {}", table);
                    }
                    println!("База данных успешно очищена.");
                }
            }
            DbCommand::Query {
                db_type,
                company_id,
                table,
                filter,
            } => {
                let db = get_db_connection(db_type, *company_id, &config).await?;
                let mut query_str = format!("SELECT * FROM \"{}\"", table);
                if let Some(f) = filter {
                    query_str.push_str(" WHERE ");
                    query_str.push_str(f);
                }

                println!("Выполнение запроса: {}", query_str);
                let results = db
                    .query_all(Statement::from_string(
                        db.get_database_backend(),
                        query_str,
                    ))
                    .await?;

                if results.is_empty() {
                    println!("[]");
                    return Ok(());
                }

                // Вручную конвертируем QueryResult в serde_json::Value
                let mut json_results: Vec<JsonValueSerde> = Vec::new();
                for row in results {
                    let mut map = Map::new();
                    for col in row.column_names() {
                        let value: JsonValue = row.try_get("", col.as_str()).unwrap_or(JsonValue::Null);
                        map.insert(col.to_string(), value.into());
                    }
                    json_results.push(JsonValueSerde::Object(map));
                }

                let pretty_json = serde_json::to_string_pretty(&json_results)?;
                println!("{}", pretty_json);
            }
            DbCommand::Update {
                db_type,
                company_id,
                table,
                id,
                data,
            } => {
                let db = get_db_connection(db_type, *company_id, &config).await?;
                let parsed_data: JsonValueSerde = serde_json::from_str(data)?;

                let obj = match parsed_data.as_object() {
                    Some(o) => o,
                    None => return Err("Данные должны быть JSON-объектом.".into()),
                };

                let mut set_clauses = Vec::new();
                let mut values: Vec<Value> = Vec::new();

                for (key, value) in obj {
                    set_clauses.push(format!("\"{}\" = ${}", key, values.len() + 1));
                    values.push(value.clone().into());
                }

                if set_clauses.is_empty() {
                    return Err("Нет данных для обновления.".into());
                }

                values.push((*id).into());
                
                let query_str = format!(
                    "UPDATE \"{}\" SET {} WHERE id = ${}",
                    table,
                    set_clauses.join(", "),
                    values.len()
                );
                
                let stmt = Statement::from_sql_and_values(
                    db.get_database_backend(),
                    &query_str,
                    values,
                );

                println!("Выполнение запроса: {}", stmt.sql);
                let result = db.execute(stmt).await?;

                if result.rows_affected() > 0 {
                    println!("Успешно обновлено {} строк.", result.rows_affected());
                } else {
                    println!("Запись с id = {} не найдена или данные не изменились.", id);
                }
            }
        },
    }

    Ok(())
}