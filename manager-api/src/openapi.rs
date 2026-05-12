use serde_json::{json, Value};

const MANAGER_API_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn document() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Dune Manager API",
            "version": MANAGER_API_VERSION,
            "description": "Authenticated control plane for a Dune Awakening dedicated server VM."
        },
        "servers": [
            { "url": "/" }
        ],
        "security": [
            { "bearerAuth": [] }
        ],
        "components": {
            "securitySchemes": {
                "bearerAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "opaque"
                }
            },
            "responses": {
                "Unauthorized": {
                    "description": "Missing or invalid manager token"
                },
                "Error": {
                    "description": "Manager API error",
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "error": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        },
        "paths": {
            "/health": {
                "get": {
                    "summary": "Health check",
                    "security": [],
                    "responses": { "200": { "description": "Manager API is alive" } }
                }
            },
            "/api/auth/login": {
                "post": {
                    "summary": "Sign in with the Self-Host Service Token",
                    "security": [],
                    "responses": { "200": { "description": "Session cookie set" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/auth/logout": {
                "post": {
                    "summary": "Clear browser session cookie",
                    "responses": { "200": { "description": "Session cleared" } }
                }
            },
            "/api/auth/session": {
                "get": {
                    "summary": "Current browser/API session",
                    "responses": { "200": { "description": "Session metadata" } }
                }
            },
            "/api/status": {
                "get": {
                    "summary": "Cluster and manager status",
                    "responses": { "200": { "description": "Status summary" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/overview": {
                "get": {
                    "summary": "Aggregated dashboard data for the Manager UI",
                    "responses": { "200": { "description": "Overview data" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/manager/self": {
                "get": {
                    "summary": "Manager process metadata",
                    "responses": { "200": { "description": "Manager process metadata" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/manager/logs": {
                "get": {
                    "summary": "Read redacted Manager API service logs",
                    "parameters": [{ "name": "tail", "in": "query", "required": false, "schema": { "type": "integer", "minimum": 1, "maximum": 5000 } }],
                    "responses": { "200": { "description": "Redacted Manager API log tail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups": {
                "get": {
                    "summary": "List battlegroups",
                    "responses": { "200": { "description": "BattleGroup summaries" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}": {
                "get": {
                    "summary": "Get battlegroup details",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "BattleGroup detail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}/raw": {
                "get": {
                    "summary": "Get redacted raw battlegroup JSON",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "Redacted raw BattleGroup resource" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}/start": {
                "post": {
                    "summary": "Start a battlegroup",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "Updated BattleGroup detail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}/stop": {
                "post": {
                    "summary": "Stop a battlegroup",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "Updated BattleGroup detail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}/restart": {
                "post": {
                    "summary": "Restart a battlegroup",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "Updated BattleGroup detail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}/layout": {
                "get": {
                    "summary": "Get user-facing world layout",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "World layout" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                },
                "put": {
                    "summary": "Update user-facing world layout",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "Updated world layout" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/battlegroups/{namespace}/{name}/settings": {
                "patch": {
                    "summary": "Update safe battlegroup settings",
                    "parameters": [namespace_parameter(), name_parameter()],
                    "responses": { "200": { "description": "Updated BattleGroup detail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/pods": {
                "get": {
                    "summary": "List pods",
                    "responses": { "200": { "description": "Pod summaries" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/services": {
                "get": {
                    "summary": "List services",
                    "responses": { "200": { "description": "Service summaries" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/workloads": {
                "get": {
                    "summary": "List pods and services",
                    "responses": { "200": { "description": "Workload summary" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/events": {
                "get": {
                    "summary": "List recent Kubernetes namespace events",
                    "parameters": [
                        { "name": "tail", "in": "query", "required": false, "schema": { "type": "integer", "minimum": 1, "maximum": 500 } }
                    ],
                    "responses": { "200": { "description": "Recent event timeline" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/storage": {
                "get": {
                    "summary": "List persistent volume claims",
                    "responses": { "200": { "description": "Persistent volume claim summaries" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/database-maintenance": {
                "get": {
                    "summary": "List database backup, restore, migration, schedule, and operation resources",
                    "responses": { "200": { "description": "Database maintenance summary" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/database-maintenance/backups": {
                "post": {
                    "summary": "Create a manual database backup",
                    "responses": { "200": { "description": "Created DatabaseBackup resource" }, "400": { "$ref": "#/components/responses/Error" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/database-maintenance/physical-backups/enable": {
                "post": {
                    "summary": "Enable physical database backups on the battlegroup",
                    "responses": { "200": { "description": "Updated database maintenance summary" }, "400": { "$ref": "#/components/responses/Error" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/logs": {
                "get": {
                    "summary": "Read pod logs",
                    "parameters": [
                        { "name": "pod", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "container", "in": "query", "required": false, "schema": { "type": "string" } },
                        { "name": "tail", "in": "query", "required": false, "schema": { "type": "integer", "minimum": 1, "maximum": 2000 } }
                    ],
                    "responses": { "200": { "description": "Redacted log lines" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/logs/export": {
                "get": {
                    "summary": "Export redacted tail logs for all pod containers",
                    "parameters": [
                        { "name": "tail", "in": "query", "required": false, "schema": { "type": "integer", "minimum": 1, "maximum": 5000 } }
                    ],
                    "responses": { "200": { "description": "Redacted log export bundle" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/logs/stream": {
                "get": {
                    "summary": "Stream pod logs over websocket",
                    "parameters": [
                        { "name": "pod", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "container", "in": "query", "required": false, "schema": { "type": "string" } },
                        { "name": "tail", "in": "query", "required": false, "schema": { "type": "integer", "minimum": 1, "maximum": 5000 } }
                    ],
                    "responses": { "101": { "description": "WebSocket log stream" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/config/user-settings": {
                "get": {
                    "summary": "List editable user settings files",
                    "responses": { "200": { "description": "Editable file catalog" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/config/user-settings/{file}": {
                "get": {
                    "summary": "Read UserEngine.ini or UserGame.ini",
                    "parameters": [{ "name": "file", "in": "path", "required": true, "schema": { "type": "string", "enum": ["engine", "game"] } }],
                    "responses": { "200": { "description": "Settings file content and parsed sections" }, "401": { "$ref": "#/components/responses/Unauthorized" }, "404": { "$ref": "#/components/responses/Error" } }
                },
                "put": {
                    "summary": "Replace UserEngine.ini or UserGame.ini",
                    "parameters": [{ "name": "file", "in": "path", "required": true, "schema": { "type": "string", "enum": ["engine", "game"] } }],
                    "responses": { "200": { "description": "Updated settings file content and parsed sections" }, "401": { "$ref": "#/components/responses/Unauthorized" }, "404": { "$ref": "#/components/responses/Error" } }
                }
            },
            "/api/config/user-settings/{file}/preview": {
                "post": {
                    "summary": "Preview changes to UserEngine.ini or UserGame.ini",
                    "parameters": [settings_file_parameter()],
                    "responses": { "200": { "description": "Line diff preview without writing the file" }, "401": { "$ref": "#/components/responses/Unauthorized" }, "404": { "$ref": "#/components/responses/Error" } }
                }
            },
            "/api/config/user-settings/{file}/backups": {
                "get": {
                    "summary": "List settings file backups",
                    "parameters": [settings_file_parameter()],
                    "responses": { "200": { "description": "Available settings backups" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                },
                "post": {
                    "summary": "Create a settings file backup",
                    "parameters": [settings_file_parameter()],
                    "responses": { "200": { "description": "Created settings backup" }, "401": { "$ref": "#/components/responses/Unauthorized" }, "404": { "$ref": "#/components/responses/Error" } }
                }
            },
            "/api/config/user-settings/{file}/backups/{backup}/restore": {
                "post": {
                    "summary": "Restore a settings file backup",
                    "parameters": [
                        settings_file_parameter(),
                        { "name": "backup", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": { "200": { "description": "Restored settings file" }, "401": { "$ref": "#/components/responses/Unauthorized" }, "404": { "$ref": "#/components/responses/Error" } }
                }
            },
            "/api/director/capabilities": {
                "get": {
                    "summary": "List Director proxy capabilities",
                    "responses": { "200": { "description": "Director capability list" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/director/battlegroup": director_get("Director battlegroup runtime data"),
            "/api/director/players/summary": director_get("Director player summary"),
            "/api/director/players": {
                "get": {
                    "summary": "Director player lists",
                    "description": "Fast by default. Set full=true to include slower in-transit, grace-period, completion, and queued buckets.",
                    "parameters": [
                        { "name": "full", "in": "query", "required": false, "schema": { "type": "boolean", "default": false } }
                    ],
                    "responses": {
                        "200": { "description": "Director player lists" },
                        "401": { "$ref": "#/components/responses/Unauthorized" },
                        "502": { "$ref": "#/components/responses/Error" }
                    }
                }
            },
            "/api/director/maps": director_get("Director map summaries"),
            "/api/director/config/fls": director_config_path("FLS report settings"),
            "/api/director/config/character-transfer": director_config_path("Character transfer settings"),
            "/api/director/config/maps/{mapName}/override": {
                "get": {
                    "summary": "Get a Director map override editor payload",
                    "parameters": [map_parameter()],
                    "responses": { "200": { "description": "Map config detail" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                },
                "post": {
                    "summary": "Update a Director map override",
                    "parameters": [map_parameter()],
                    "responses": { "200": { "description": "Director response" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                },
                "delete": {
                    "summary": "Clear a Director map override",
                    "parameters": [map_parameter()],
                    "responses": { "200": { "description": "Director response" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            },
            "/api/director/v0/{path}": {
                "get": { "summary": "Allowlisted Director API proxy", "parameters": [path_parameter()], "responses": { "200": { "description": "Director response" } } },
                "post": { "summary": "Allowlisted Director API proxy", "parameters": [path_parameter()], "responses": { "200": { "description": "Director response" } } }
            },
            "/api/telemetry": {
                "get": {
                    "summary": "Telemetry websocket",
                    "responses": { "101": { "description": "WebSocket telemetry stream" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
                }
            }
        }
    })
}

fn namespace_parameter() -> Value {
    json!({ "name": "namespace", "in": "path", "required": true, "schema": { "type": "string" } })
}

fn name_parameter() -> Value {
    json!({ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } })
}

fn map_parameter() -> Value {
    json!({ "name": "mapName", "in": "path", "required": true, "schema": { "type": "string" } })
}

fn settings_file_parameter() -> Value {
    json!({ "name": "file", "in": "path", "required": true, "schema": { "type": "string", "enum": ["engine", "game"] } })
}

fn path_parameter() -> Value {
    json!({ "name": "path", "in": "path", "required": true, "schema": { "type": "string" } })
}

fn director_get(summary: &str) -> Value {
    json!({
        "get": {
            "summary": summary,
            "responses": {
                "200": { "description": "Director response" },
                "401": { "$ref": "#/components/responses/Unauthorized" },
                "502": { "$ref": "#/components/responses/Error" }
            }
        }
    })
}

fn director_config_path(summary: &str) -> Value {
    json!({
        "get": {
            "summary": format!("Fetch {summary}"),
            "responses": { "200": { "description": "Director config" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
        },
        "post": {
            "summary": format!("Update {summary}"),
            "responses": { "200": { "description": "Director response" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
        },
        "delete": {
            "summary": format!("Clear {summary} overrides"),
            "responses": { "200": { "description": "Director response" }, "401": { "$ref": "#/components/responses/Unauthorized" } }
        }
    })
}
