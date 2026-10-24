pub struct Config {
    pub redis_url: String,
    pub discord_token: String,
    pub postgres_url: String,
    pub sqlite_path: String,
    pub github_token: Option<String>,
}

impl Config {
    /// Load configuration from the environment and an optional `.env` file.
    ///
    /// # Panics
    /// Panics if `REDIS_URL`, `DISCORD_TOKEN`, `POSTGRES_URL`, or `SQLITE_PATH`
    /// is missing or contains invalid Unicode.
    #[must_use]
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        let redis_url = std::env::var("REDIS_URL").expect("Expected REDIS_URL in environment");
        let discord_token =
            std::env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
        let postgres_url =
            std::env::var("POSTGRES_URL").expect("Expected POSTGRES_URL in environment");
        let sqlite_path =
            std::env::var("SQLITE_PATH").expect("Expected SQLITE_PATH in environment");
        let github_token = std::env::var("GITHUB_TOKEN").ok();

        Self {
            redis_url,
            discord_token,
            postgres_url,
            sqlite_path,
            github_token,
        }
    }
}
