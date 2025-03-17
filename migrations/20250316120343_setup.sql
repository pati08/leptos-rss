CREATE TABLE sessions {
    id          SERIAL PRIMARY KEY,
    authkey     VARCHAR(100) NOT NULL,
    user_agent  VARCHAR(100) NOT NULL
};

CREATE TABLE users {
    id          SERIAL PRIMARY KEY,
    name        VARCHAR(100) NOT NULL,
    loginhash   VARCHAR(100) NOT NULL
}
