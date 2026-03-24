# Asistent Virtual

To run the production container, you have to configure these things:

1. **Volumes** - create these volumes
* `docker volume create db_data`
* `docker volume create backend_documents`

2. **Secrets** - Inside `./secrets`, generate these secrets as files:
* `jwt_access_secret.txt`
* `jwt_refresh_secret.txt`
* `postgres_password.txt`
