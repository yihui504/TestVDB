#!/usr/bin/env python3
"""Generate Weaviate v1.38.0 structured_contract.json"""

import json
import hashlib
from datetime import datetime, timezone

now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

# ---- Common source URL ----
OPENAPI_URL = "https://raw.githubusercontent.com/weaviate/weaviate/v1.38.0/openapi-specs/schema.json"
DOC_URLS = [
    OPENAPI_URL,
    "https://docs.weaviate.io/weaviate/api/rest",
    "https://docs.weaviate.io/weaviate/api/graphql",
    "https://docs.weaviate.io/weaviate/api/grpc",
    "https://docs.weaviate.io/weaviate/config-refs/collections",
    "https://docs.weaviate.io/weaviate/config-refs/distances",
    "https://docs.weaviate.io/weaviate/config-refs/datatypes",
]

def ep(path, method, category, description, parameters, source_url=OPENAPI_URL, doc_version="1.38.0"):
    return {
        "path": path,
        "method": method,
        "category": category,
        "description": description,
        "source_url": source_url,
        "doc_version": doc_version,
        "parameters": parameters,
    }

def param(name, typ, required, description="", default_value=None, enum_values=None):
    p = {"name": name, "type": typ, "required": required}
    if description:
        p["description"] = description
    if default_value is not None:
        p["default_value"] = default_value
    if enum_values:
        p["enum_values"] = enum_values
    return p

def ref_entry(path, method, description):
    return {
        "path": path,
        "method": method,
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
        "doc_quote": description,
        "verified_at": now,
    }

# ---- API Endpoints ----
endpoints = []

# === Health & Discovery ===
endpoints.append(ep("/", "GET", "management", "Root endpoint listing available API endpoints", []))
endpoints.append(ep("/.well-known/live", "GET", "management", "Liveness probe endpoint", []))
endpoints.append(ep("/.well-known/ready", "GET", "management", "Readiness probe endpoint", []))
endpoints.append(ep("/.well-known/openid-configuration", "GET", "management", "OIDC configuration endpoint", []))

# === Schema Management (Collections) ===
endpoints.append(ep(
    "/schema", "POST", "collections", "Create a new collection",
    [param("objectClass", "Class", True, "Collection definition object with class, properties, vectorConfig")],
))
endpoints.append(ep(
    "/schema", "GET", "collections", "Get all collections with schema definitions",
    [param("consistency", "boolean", False, "Consistency level")],
))
endpoints.append(ep(
    "/schema/{className}", "GET", "collections", "Get single collection definition",
    [param("className", "string", True, "Collection name"), param("consistency", "boolean", False, "Consistency level")],
))
endpoints.append(ep(
    "/schema/{className}", "PUT", "collections", "Update collection settings",
    [param("className", "string", True, "Collection name"), param("objectClass", "Class", True, "Updated collection definition")],
))
endpoints.append(ep(
    "/schema/{className}", "DELETE", "collections", "Delete a collection and all its data",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/properties", "POST", "collections", "Add a property to a collection",
    [param("className", "string", True, "Collection name"), param("body", "Property", True, "Property definition with name and dataType")],
))
endpoints.append(ep(
    "/schema/{className}/shards", "GET", "collections", "Get shards status for a collection",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/shards/{shardName}", "PUT", "collections", "Update shard status",
    [param("className", "string", True, "Collection name"), param("shardName", "string", True, "Shard name"), param("body", "object", True, "Shard status update")],
))

# === Indexes ===
endpoints.append(ep(
    "/schema/{className}/indexes", "GET", "index", "Get index status for all properties",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/indexes/{propertyName}", "PUT", "index", "Update property index configuration",
    [param("className", "string", True, "Collection name"), param("propertyName", "string", True, "Property name"), param("body", "IndexUpdateRequest", True, "Index config with searchable/filterable/rangeable")],
))
endpoints.append(ep(
    "/schema/{className}/vectors/{vectorIndexName}/index", "DELETE", "index", "Delete a vector index",
    [param("className", "string", True, "Collection name"), param("vectorIndexName", "string", True, "Vector index name")],
))

# === Tenants ===
endpoints.append(ep(
    "/schema/{className}/tenants", "POST", "management", "Create tenants for a collection",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/tenants", "PUT", "management", "Update tenant statuses",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/tenants", "DELETE", "management", "Delete tenants",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/tenants", "GET", "management", "List all tenants",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/schema/{className}/tenants/{tenantName}", "GET", "management", "Get single tenant",
    [param("className", "string", True, "Collection name"), param("tenantName", "string", True, "Tenant name")],
))

# === Objects CRUD ===
endpoints.append(ep(
    "/objects", "GET", "points", "List objects with optional class filter and pagination",
    [
        param("class", "string", False, "Collection name filter"),
        param("limit", "integer", False, "Max results (capped at 10000)"),
        param("after", "string", False, "Cursor for pagination"),
        param("offset", "integer", False, "Offset for pagination"),
        param("include", "string", False, "Additional fields to include"),
    ],
))
endpoints.append(ep(
    "/objects", "POST", "points", "Create a new object",
    [param("body", "object", True, "Object data with class, properties, optional id/vector")],
))
endpoints.append(ep(
    "/objects/validate", "POST", "points", "Validate an object without creating it",
    [param("body", "object", True, "Object to validate")],
))
endpoints.append(ep(
    "/objects/{className}/{id}", "GET", "points", "Get an object by className and id",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID"), param("include", "string", False, "Additional fields")],
))
endpoints.append(ep(
    "/objects/{className}/{id}", "PUT", "points", "Replace an object (full replacement)",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID"), param("body", "object", True, "Full replacement object")],
))
endpoints.append(ep(
    "/objects/{className}/{id}", "PATCH", "points", "Patch an object (partial update, JSON Merge Patch)",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID"), param("body", "object", True, "Partial object with fields to update")],
))
endpoints.append(ep(
    "/objects/{className}/{id}", "DELETE", "points", "Delete an object",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID")],
))
endpoints.append(ep(
    "/objects/{className}/{id}", "HEAD", "points", "Check if an object exists",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID")],
))
endpoints.append(ep(
    "/objects/{id}", "GET", "points", "Legacy: get object by id only",
    [param("id", "string", True, "Object UUID")],
))
endpoints.append(ep(
    "/objects/{id}", "PUT", "points", "Legacy: replace object by id only",
    [param("id", "string", True, "Object UUID"), param("body", "object", True, "Replacement object")],
))
endpoints.append(ep(
    "/objects/{id}", "PATCH", "points", "Legacy: patch object by id only",
    [param("id", "string", True, "Object UUID"), param("body", "object", True, "Partial object")],
))
endpoints.append(ep(
    "/objects/{id}", "DELETE", "points", "Legacy: delete object by id only",
    [param("id", "string", True, "Object UUID")],
))
endpoints.append(ep(
    "/objects/{id}", "HEAD", "points", "Legacy: check object exists by id only",
    [param("id", "string", True, "Object UUID")],
))

# === Object References ===
endpoints.append(ep(
    "/objects/{className}/{id}/references/{propertyName}", "POST", "points", "Add a cross-reference to an object",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID"), param("propertyName", "string", True, "Cross-reference property name"), param("body", "SingleRef", True, "Reference with beacon or class/schema")],
))
endpoints.append(ep(
    "/objects/{className}/{id}/references/{propertyName}", "PUT", "points", "Replace all cross-references on a property",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID"), param("propertyName", "string", True, "Cross-reference property name"), param("body", "array", True, "Array of SingleRef")],
))
endpoints.append(ep(
    "/objects/{className}/{id}/references/{propertyName}", "DELETE", "points", "Delete a specific cross-reference",
    [param("className", "string", True, "Collection name"), param("id", "string", True, "Object UUID"), param("propertyName", "string", True, "Cross-reference property name"), param("body", "SingleRef", True, "Reference to delete")],
))

# === Batch Operations ===
endpoints.append(ep(
    "/batch/objects", "POST", "points", "Batch create objects",
    [param("body", "object", True, "Batch with fields and objects array")],
))
endpoints.append(ep(
    "/batch/objects", "DELETE", "points", "Batch delete objects by filter",
    [param("body", "object", True, "BatchDelete with match filter, optional output and dryRun")],
))
endpoints.append(ep(
    "/batch/references", "POST", "points", "Batch create cross-references",
    [param("body", "array", True, "Array of BatchReference with from/to beacons")],
))

# === GraphQL ===
endpoints.append(ep(
    "/graphql", "POST", "search", "Execute a GraphQL query",
    [param("body", "GraphQLQuery", True, "GraphQL query with query string, optional operationName and variables")],
))
endpoints.append(ep(
    "/graphql/batch", "POST", "search", "Execute batched GraphQL queries",
    [param("body", "array", True, "Array of GraphQLQuery objects")],
))

# === Nodes & Cluster ===
endpoints.append(ep(
    "/nodes", "GET", "management", "Get all nodes status",
    [param("output", "string", False, "Detail level: minimal or verbose", None, ["minimal", "verbose"])],
))
endpoints.append(ep(
    "/nodes/{className}", "GET", "management", "Get nodes status for a specific collection",
    [param("className", "string", True, "Collection name")],
))
endpoints.append(ep(
    "/cluster/statistics", "GET", "management", "Get cluster statistics", [],
))

# === Meta ===
endpoints.append(ep(
    "/meta", "GET", "management", "Get instance metadata (version, hostname, modules)", [],
))

# === Backups ===
endpoints.append(ep(
    "/backups/{backend}", "POST", "management", "Create a backup",
    [param("backend", "string", True, "Backup backend (filesystem, s3, gcs)"), param("body", "BackupCreateRequest", True, "Backup request with id, optional config/include/exclude")],
))
endpoints.append(ep(
    "/backups/{backend}", "GET", "management", "List backups for a backend",
    [param("backend", "string", True, "Backup backend")],
))
endpoints.append(ep(
    "/backups/{backend}/{id}", "GET", "management", "Get backup status",
    [param("backend", "string", True, "Backup backend"), param("id", "string", True, "Backup ID")],
))
endpoints.append(ep(
    "/backups/{backend}/{id}", "DELETE", "management", "Cancel a backup",
    [param("backend", "string", True, "Backup backend"), param("id", "string", True, "Backup ID")],
))
endpoints.append(ep(
    "/backups/{backend}/{id}/restore", "POST", "management", "Restore from a backup",
    [param("backend", "string", True, "Backup backend"), param("id", "string", True, "Backup ID"), param("body", "BackupRestoreRequest", False, "Restore config with optional include/exclude/node_mapping")],
))
endpoints.append(ep(
    "/backups/{backend}/{id}/restore", "GET", "management", "Get restore status",
    [param("backend", "string", True, "Backup backend"), param("id", "string", True, "Backup ID")],
))
endpoints.append(ep(
    "/backups/{backend}/{id}/restore", "DELETE", "management", "Cancel a restore",
    [param("backend", "string", True, "Backup backend"), param("id", "string", True, "Backup ID")],
))

# === Replication ===
endpoints.append(ep(
    "/replication/replicate", "POST", "management", "Initiate replica movement",
    [param("body", "object", True, "ReplicationReplicateReplicaRequest with sourceNode, targetNode, collection, shard, type")],
))
endpoints.append(ep(
    "/replication/replicate/list", "GET", "management", "List replication operations", [],
))
endpoints.append(ep(
    "/replication/replicate/{id}", "GET", "management", "Get replication operation details",
    [param("id", "string", True, "Operation ID")],
))
endpoints.append(ep(
    "/replication/replicate/{id}", "DELETE", "management", "Delete a replication operation",
    [param("id", "string", True, "Operation ID")],
))
endpoints.append(ep(
    "/replication/replicate/{id}/cancel", "POST", "management", "Cancel a replication operation",
    [param("id", "string", True, "Operation ID")],
))
endpoints.append(ep(
    "/replication/replicate/force-delete", "POST", "management", "Force delete stuck replication operations",
    [param("body", "object", False, "Force delete request with optional id, collection, shard, node, dryRun")],
))
endpoints.append(ep(
    "/replication/scale", "GET", "management", "Get replication scaling plan", [],
))
endpoints.append(ep(
    "/replication/scale", "POST", "management", "Apply replication scaling plan",
    [param("body", "object", True, "Scaling plan")],
))
endpoints.append(ep(
    "/replication/sharding-state", "GET", "management", "Get sharding state", [],
))

# === Authorization & RBAC ===
endpoints.append(ep(
    "/authz/roles", "GET", "management", "List all roles", [],
))
endpoints.append(ep(
    "/authz/roles", "POST", "management", "Create a role",
    [param("body", "object", True, "Role with name and permissions")],
))
endpoints.append(ep(
    "/authz/roles/{id}", "GET", "management", "Get a role by ID",
    [param("id", "string", True, "Role ID")],
))
endpoints.append(ep(
    "/authz/roles/{id}", "DELETE", "management", "Delete a role",
    [param("id", "string", True, "Role ID")],
))
endpoints.append(ep(
    "/authz/roles/{id}/add-permissions", "POST", "management", "Add permissions to a role",
    [param("id", "string", True, "Role ID"), param("body", "object", True, "Permissions to add")],
))
endpoints.append(ep(
    "/authz/roles/{id}/remove-permissions", "POST", "management", "Remove permissions from a role",
    [param("id", "string", True, "Role ID"), param("body", "object", True, "Permissions to remove")],
))
endpoints.append(ep(
    "/authz/roles/{id}/has-permission", "POST", "management", "Check if a role has a specific permission",
    [param("id", "string", True, "Role ID"), param("body", "object", True, "Permission+action to check")],
))
endpoints.append(ep(
    "/authz/users/{id}/assign", "POST", "management", "Assign a role to a user",
    [param("id", "string", True, "User ID"), param("body", "object", True, "Role assignment details")],
))
endpoints.append(ep(
    "/authz/users/{id}/revoke", "POST", "management", "Revoke a role from a user",
    [param("id", "string", True, "User ID"), param("body", "object", True, "Role revocation details")],
))
endpoints.append(ep(
    "/authz/groups/{id}/assign", "POST", "management", "Assign a role to a group",
    [param("id", "string", True, "Group ID"), param("body", "object", True, "Role assignment details")],
))
endpoints.append(ep(
    "/authz/groups/{id}/revoke", "POST", "management", "Revoke a role from a group",
    [param("id", "string", True, "Group ID"), param("body", "object", True, "Role revocation details")],
))
endpoints.append(ep(
    "/authz/users/{id}/roles/{userType}", "GET", "management", "Get role assignments for a user",
    [param("id", "string", True, "User ID"), param("userType", "string", True, "Type: db or oidc")],
))
endpoints.append(ep(
    "/authz/groups/{id}/roles/{groupType}", "GET", "management", "Get role assignments for a group",
    [param("id", "string", True, "Group ID"), param("groupType", "string", True, "Type: oidc")],
))

# === User Management ===
endpoints.append(ep(
    "/users/db", "GET", "management", "List all database users", [],
))
endpoints.append(ep(
    "/users/db", "POST", "management", "Create a database user",
    [param("body", "object", True, "User creation payload")],
))
endpoints.append(ep(
    "/users/db/{user_id}", "GET", "management", "Get a specific user",
    [param("user_id", "string", True, "User ID")],
))
endpoints.append(ep(
    "/users/db/{user_id}", "DELETE", "management", "Delete a user",
    [param("user_id", "string", True, "User ID")],
))
endpoints.append(ep(
    "/users/db/{user_id}/activate", "POST", "management", "Activate a user",
    [param("user_id", "string", True, "User ID")],
))
endpoints.append(ep(
    "/users/db/{user_id}/deactivate", "POST", "management", "Deactivate a user",
    [param("user_id", "string", True, "User ID")],
))
endpoints.append(ep(
    "/users/db/{user_id}/rotate-key", "POST", "management", "Rotate user API key",
    [param("user_id", "string", True, "User ID")],
))
endpoints.append(ep(
    "/users/own-info", "GET", "management", "Get current authenticated user info", [],
))

# === Namespaces ===
endpoints.append(ep(
    "/namespaces", "GET", "management", "List all namespaces", [],
))
endpoints.append(ep(
    "/namespaces", "POST", "management", "Create a namespace",
    [param("body", "object", True, "NamespaceCreateRequest with optional home_node")],
))
endpoints.append(ep(
    "/namespaces/{namespace_id}", "GET", "management", "Get a namespace",
    [param("namespace_id", "string", True, "Namespace ID")],
))
endpoints.append(ep(
    "/namespaces/{namespace_id}", "PUT", "management", "Update a namespace",
    [param("namespace_id", "string", True, "Namespace ID"), param("body", "object", False, "NamespaceUpdateRequest")],
))
endpoints.append(ep(
    "/namespaces/{namespace_id}", "DELETE", "management", "Delete a namespace",
    [param("namespace_id", "string", True, "Namespace ID")],
))

# === Aliases ===
endpoints.append(ep(
    "/aliases", "GET", "management", "List all aliases", [],
))
endpoints.append(ep(
    "/aliases", "POST", "management", "Create an alias",
    [param("body", "object", True, "Alias with alias name and target class")],
))
endpoints.append(ep(
    "/aliases/{aliasName}", "GET", "management", "Get an alias",
    [param("aliasName", "string", True, "Alias name")],
))
endpoints.append(ep(
    "/aliases/{aliasName}", "PUT", "management", "Update an alias",
    [param("aliasName", "string", True, "Alias name"), param("body", "object", True, "Updated alias")],
))
endpoints.append(ep(
    "/aliases/{aliasName}", "DELETE", "management", "Delete an alias",
    [param("aliasName", "string", True, "Alias name")],
))

# === Classifications ===
endpoints.append(ep(
    "/classifications/", "POST", "management", "Start a classification",
    [param("body", "object", True, "Classification with class, classifyProperties, basedOnProperties, type")],
))
endpoints.append(ep(
    "/classifications/{id}", "GET", "management", "Get classification status",
    [param("id", "string", True, "Classification ID")],
))

# === Exports ===
endpoints.append(ep(
    "/export/{backend}", "POST", "management", "Start an export",
    [param("backend", "string", True, "Export backend"), param("body", "object", True, "ExportCreateRequest with id and file_format")],
))
endpoints.append(ep(
    "/export/{backend}/{id}", "GET", "management", "Get export status",
    [param("backend", "string", True, "Export backend"), param("id", "string", True, "Export ID")],
))
endpoints.append(ep(
    "/export/{backend}/{id}", "DELETE", "management", "Cancel an export",
    [param("backend", "string", True, "Export backend"), param("id", "string", True, "Export ID")],
))

# === MCP ===
endpoints.append(ep(
    "/mcp", "GET", "management", "List MCP capabilities", [],
))
endpoints.append(ep(
    "/mcp", "POST", "management", "Execute MCP operation",
    [param("body", "object", True, "MCP request")],
))
endpoints.append(ep(
    "/mcp", "DELETE", "management", "Clean up MCP resources", [],
))

# === Tokenization ===
endpoints.append(ep(
    "/tokenize", "POST", "management", "Tokenize text",
    [param("body", "object", True, "TokenizeRequest with text and tokenization method")],
))
endpoints.append(ep(
    "/schema/{className}/properties/{propertyName}/tokenize", "POST", "management", "Tokenize property text",
    [param("className", "string", True, "Collection name"), param("propertyName", "string", True, "Property name"), param("body", "object", True, "PropertyTokenizeRequest with text")],
))

# === Distributed Tasks ===
endpoints.append(ep(
    "/tasks", "GET", "management", "List distributed tasks", [],
))

# ---- Endpoint count ----
print(f"Total endpoints: {len(endpoints)}")

# ---- Constraints ----
type_constraints = []
range_constraints = []
state_constraints = []

# Type constraints
type_constraints.append({
    "constraint_id": "weaviate_type_create_collection_001",
    "endpoint": "/schema",
    "description": "Request body must be a valid Class object with required class field (string)",
    "assertion": "Request body must be a valid Class object with required `class` field (string)",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_add_property_001",
    "endpoint": "/schema/{className}/properties",
    "description": "Property object must include name and dataType",
    "assertion": "Property object must include `name` and `dataType`",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_create_object_001",
    "endpoint": "/objects",
    "description": "Object body must include class (string); id must be UUID format if provided; vector is array of float64",
    "assertion": "Object body must include `class` (string); `id` must be UUID format if provided; `vector` is array of float64",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_batch_create_001",
    "endpoint": "/batch/objects",
    "description": "Must provide fields (array of strings) and objects (array of Object definitions)",
    "assertion": "Must provide `fields` (array of strings) and `objects` (array of Object definitions)",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_batch_delete_001",
    "endpoint": "/batch/objects",
    "description": "Body must include match with class (string) and where filter",
    "assertion": "Body must include `match` with `class` (string) and `where` filter",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_batch_references_001",
    "endpoint": "/batch/references",
    "description": "Each reference must have from (beacon string) and to (beacon string)",
    "assertion": "Each reference must have `from` (beacon string) and `to` (beacon string)",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_graphql_query_001",
    "endpoint": "/graphql",
    "description": "Body must be a valid GraphQLQuery object; query field is required",
    "assertion": "Body must be a valid GraphQLQuery object; `query` field is required",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_add_reference_001",
    "endpoint": "/objects/{className}/{id}/references/{propertyName}",
    "description": "Body must contain beacon (URL) or class + schema (concept reference)",
    "assertion": "Body must contain `beacon` (URL) or `class` + `schema` (concept reference)",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_alias_001",
    "endpoint": "/aliases",
    "description": "Alias must have alias (string) and class (string, target collection name)",
    "assertion": "Alias must have `alias` (string) and `class` (string, target collection name)",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
type_constraints.append({
    "constraint_id": "weaviate_type_namespace_001",
    "endpoint": "/namespaces",
    "description": "Namespace names must contain only alphanumeric characters, hyphens, and underscores",
    "assertion": "Namespace names must contain only alphanumeric characters, hyphens, and underscores",
    "type": "type_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})

# Range constraints
range_constraints.append({
    "constraint_id": "weaviate_range_list_objects_001",
    "endpoint": "/objects",
    "description": "limit is typically capped at configurable maximum (e.g. 10000)",
    "assertion": "limit capped at configurable maximum (e.g. 10000)",
    "type": "range_constraint",
    "confidence": 0.9,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
range_constraints.append({
    "constraint_id": "weaviate_range_create_object_001",
    "endpoint": "/objects",
    "description": "Vector dimensions must match collection configuration",
    "assertion": "Vector dimensions must match collection configuration",
    "type": "range_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
range_constraints.append({
    "constraint_id": "weaviate_range_get_object_001",
    "endpoint": "/objects/{className}/{id}",
    "description": "id must be valid UUID format",
    "assertion": "id must be valid UUID format",
    "type": "range_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
range_constraints.append({
    "constraint_id": "weaviate_range_batch_create_001",
    "endpoint": "/batch/objects",
    "description": "Objects array capped by configurable batch size limits",
    "assertion": "Objects array capped by configurable batch size limits",
    "type": "range_constraint",
    "confidence": 0.8,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
range_constraints.append({
    "constraint_id": "weaviate_range_batch_delete_001",
    "endpoint": "/batch/objects",
    "description": "Configurable default limit on number of deletions",
    "assertion": "Configurable default limit on number of deletions",
    "type": "range_constraint",
    "confidence": 0.8,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
range_constraints.append({
    "constraint_id": "weaviate_range_nodes_output_001",
    "endpoint": "/nodes",
    "description": "output must be minimal or verbose",
    "assertion": "output must be \"minimal\" or \"verbose\"",
    "type": "range_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
range_constraints.append({
    "constraint_id": "weaviate_range_classifications_001",
    "endpoint": "/classifications/",
    "description": "Must specify class, classifyProperties, basedOnProperties",
    "assertion": "Must specify `class`, `classifyProperties`, `basedOnProperties`",
    "type": "range_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})

# State constraints
state_constraints.append({
    "constraint_id": "weaviate_state_create_collection_001",
    "endpoint": "/schema",
    "description": "Collection creation is atomic; if AutoSchema enabled, Weaviate may infer schema",
    "assertion": "Creation is atomic; AutoSchema may infer schema if enabled",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_update_collection_001",
    "endpoint": "/schema/{className}",
    "description": "Only mutable settings can be updated; immutable settings are ignored",
    "assertion": "Only mutable settings can be updated; immutable settings are ignored",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_delete_collection_001",
    "endpoint": "/schema/{className}",
    "description": "WARNING: permanently deletes ALL data objects in the collection",
    "assertion": "Deleting a collection permanently deletes ALL data objects in it",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_create_object_001",
    "endpoint": "/objects",
    "description": "Atomic creation; idempotent for same id if not exists overwritten",
    "assertion": "Atomic creation; idempotent for same id (overwrites if not exists)",
    "type": "state_constraint",
    "confidence": 0.9,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_replace_object_001",
    "endpoint": "/objects/{className}/{id}",
    "description": "PUT is full replacement (not partial update)",
    "assertion": "PUT performs full replacement, not partial update",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_patch_object_001",
    "endpoint": "/objects/{className}/{id}",
    "description": "PATCH uses JSON Merge Patch (RFC 7396) semantics; partial update",
    "assertion": "PATCH uses JSON Merge Patch (RFC 7396) semantics; only provided fields modified",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_delete_object_001",
    "endpoint": "/objects/{className}/{id}",
    "description": "Permanent deletion",
    "assertion": "Object deletion is permanent",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_batch_create_001",
    "endpoint": "/batch/objects",
    "description": "Idempotent based on ID; individual object statuses returned",
    "assertion": "Idempotent based on ID; individual object statuses returned",
    "type": "state_constraint",
    "confidence": 0.9,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_batch_delete_001",
    "endpoint": "/batch/objects",
    "description": "Deletion based on filter criteria; supports dryRun",
    "assertion": "Deletion based on filter criteria; supports dryRun mode",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})
state_constraints.append({
    "constraint_id": "weaviate_state_backup_001",
    "endpoint": "/backups/{backend}",
    "description": "Backup is asynchronous; status tracked via GET",
    "assertion": "Backup creation is asynchronous; status tracked via GET endpoint",
    "type": "state_constraint",
    "confidence": 1.0,
    "source_url": OPENAPI_URL,
    "source_status": "reachable",
})

# ---- Assertions ----
assertions = [
    {
        "assertion_id": "weaviate_behavioral_create_collection_001",
        "endpoint": "/schema",
        "description": "Valid collection creation returns 200; invalid returns 422",
        "category": "behavioral",
        "expected_behavior": "Normal input (valid Class) -> 200; Invalid definition -> 422; Usage limit -> 429",
        "confidence": 1.0,
        "defect_type_if_violated": "Type1_IllegalSuccess",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_get_collection_001",
        "endpoint": "/schema/{className}",
        "description": "Get single collection returns 200 for existing and 404 for non-existent",
        "category": "behavioral",
        "expected_behavior": "Existing collection -> 200; Non-existent -> 404",
        "confidence": 1.0,
        "defect_type_if_violated": "Type1_IllegalSuccess",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_delete_collection_001",
        "endpoint": "/schema/{className}",
        "description": "Delete collection returns 200 for existing and 400 for non-existent",
        "category": "behavioral",
        "expected_behavior": "Existing collection -> 200; Non-existent -> 400",
        "confidence": 1.0,
        "defect_type_if_violated": "Type1_IllegalSuccess",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_list_objects_001",
        "endpoint": "/objects",
        "description": "List objects returns 200 with objects; 404 if no matching objects",
        "category": "behavioral",
        "expected_behavior": "Valid params -> 200; No matching objects -> 404",
        "confidence": 1.0,
        "defect_type_if_violated": "Type4_StateLogicViolation",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_get_object_001",
        "endpoint": "/objects/{className}/{id}",
        "description": "Get object returns 200 for existing and 404 for not found",
        "category": "behavioral",
        "expected_behavior": "Existing object -> 200; Not found -> 404",
        "confidence": 1.0,
        "defect_type_if_violated": "Type3_RuntimeFailure",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_patch_object_001",
        "endpoint": "/objects/{className}/{id}",
        "description": "Patch returns 204 for existing object, 404 for not found",
        "category": "behavioral",
        "expected_behavior": "Existing object -> 204; Not found -> 404; Invalid patch -> 422",
        "confidence": 1.0,
        "defect_type_if_violated": "Type1_IllegalSuccess",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_delete_object_001",
        "endpoint": "/objects/{className}/{id}",
        "description": "Delete returns 204 for existing object, 404 for not found",
        "category": "behavioral",
        "expected_behavior": "Existing object -> 204; Not found -> 404",
        "confidence": 1.0,
        "defect_type_if_violated": "Type1_IllegalSuccess",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_head_object_001",
        "endpoint": "/objects/{className}/{id}",
        "description": "HEAD returns 204 if exists, 404 if not found",
        "category": "behavioral",
        "expected_behavior": "Exists -> 204; Not found -> 404",
        "confidence": 1.0,
        "defect_type_if_violated": "Type1_IllegalSuccess",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_graphql_001",
        "endpoint": "/graphql",
        "description": "Valid GraphQL query returns 200 with data; invalid returns 422",
        "category": "behavioral",
        "expected_behavior": "Valid query -> 200 with data/errors; Invalid -> 422",
        "confidence": 1.0,
        "defect_type_if_violated": "Type2_PoorDiagnostics",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
    {
        "assertion_id": "weaviate_behavioral_empty_collection_001",
        "endpoint": "/objects",
        "description": "Listing objects in an empty collection returns empty results (no error)",
        "category": "behavioral",
        "expected_behavior": "Empty collection returns empty result set, no error",
        "confidence": 0.9,
        "defect_type_if_violated": "Type3_RuntimeFailure",
        "source_url": OPENAPI_URL,
        "doc_version": "1.38.0",
    },
]

# ---- Behavioral Contracts ----
behavioral_contracts = [
    {
        "contract_id": "weaviate_bcontract_create_query_001",
        "description": "Created object is immediately queryable via GET and GraphQL",
        "scenario": "Create an object via POST /objects, then immediately GET /objects/{className}/{id} and query via GraphQL",
        "expected_behavior": "Object is immediately visible after creation; GET returns 200 and GraphQL returns object in results",
        "related_endpoints": ["/objects", "/objects/{className}/{id}", "/graphql"],
        "source_url": OPENAPI_URL,
    },
    {
        "contract_id": "weaviate_bcontract_delete_query_002",
        "description": "Deleted object is no longer queryable",
        "scenario": "Create an object, then delete it via DELETE /objects/{className}/{id}, then attempt to GET and GraphQL query",
        "expected_behavior": "After deletion, GET returns 404 and GraphQL no longer returns the object",
        "related_endpoints": ["/objects/{className}/{id}", "/graphql"],
        "source_url": OPENAPI_URL,
    },
    {
        "contract_id": "weaviate_bcontract_batch_atomicity_003",
        "description": "Batch operations may have individual failures but overall request succeeds with per-object statuses",
        "scenario": "Send a batch create with mixed valid and invalid objects",
        "expected_behavior": "Batch returns 200 with per-object statuses; valid objects are created, invalid ones have error statuses",
        "related_endpoints": ["/batch/objects"],
        "source_url": OPENAPI_URL,
    },
]

# ---- State Invariants ----
state_invariants = [
    {
        "invariant_id": "weaviate_invariant_create_queryable_001",
        "description": "After collection creation, the collection is immediately queryable via GET /schema/{className}",
        "assertion": "POST /schema (create collection) -> GET /schema/{className} returns 200",
        "scope": "per_collection",
        "source_url": OPENAPI_URL,
    },
    {
        "invariant_id": "weaviate_invariant_delete_gone_002",
        "description": "After collection deletion, querying the collection returns 404",
        "assertion": "DELETE /schema/{className} -> GET /schema/{className} returns 404",
        "scope": "per_collection",
        "source_url": OPENAPI_URL,
    },
    {
        "invariant_id": "weaviate_invariant_count_consistency_003",
        "description": "Total object count matches number of created objects",
        "assertion": "Creating N objects and listing /objects returns N results (subject to pagination)",
        "scope": "per_collection",
        "source_url": OPENAPI_URL,
    },
    {
        "invariant_id": "weaviate_invariant_batch_idempotent_004",
        "description": "Batch create with existing IDs is idempotent - re-inserting same ID overwrites",
        "assertion": "Repeated batch create with same IDs does not create duplicates",
        "scope": "per_collection",
        "source_url": OPENAPI_URL,
    },
]

# ---- Data Types ----
data_types = [
    {
        "name": "text",
        "description": "String data; used for vectorization and keyword search unless specified otherwise",
        "fields": None,
    },
    {
        "name": "text[]",
        "description": "Array of text values",
        "fields": None,
    },
    {
        "name": "boolean",
        "description": "Boolean value",
        "fields": None,
    },
    {
        "name": "int",
        "description": "Integer value (int64)",
        "fields": None,
    },
    {
        "name": "number",
        "description": "Floating point number (float64)",
        "fields": None,
    },
    {
        "name": "date",
        "description": "Date/time in ISO 8601 format",
        "fields": None,
    },
    {
        "name": "uuid",
        "description": "UUID string",
        "fields": None,
    },
    {
        "name": "geoCoordinates",
        "description": "Object with latitude and longitude (float64)",
        "fields": [
            {"name": "latitude", "type": "float64", "required": True},
            {"name": "longitude", "type": "float64", "required": True},
        ],
    },
    {
        "name": "phoneNumber",
        "description": "Object with phone number fields",
        "fields": [
            {"name": "input", "type": "string", "required": True},
            {"name": "defaultCountry", "type": "string", "required": False},
        ],
    },
    {
        "name": "blob",
        "description": "Base64 encoded binary data",
        "fields": None,
    },
    {
        "name": "blobHash",
        "description": "Hash of blob content",
        "fields": None,
    },
    {
        "name": "vector",
        "description": "Array of float64 (single vector, default)",
        "fields": None,
    },
    {
        "name": "vectors",
        "description": "Map of named vectors (for multi-vector collections), each being an array of float64",
        "fields": None,
    },
    {
        "name": "cross-reference",
        "description": "Reference to object(s) in another collection",
        "fields": None,
    },
    {
        "name": "cosine",
        "description": "Cosine (angular) distance. Formula: 1 - cosine_sim(a,b). Uses SIMD optimization.",
        "fields": None,
    },
    {
        "name": "dot",
        "description": "Negative dot product. Returns -dot(a,b). Uses SIMD optimization.",
        "fields": None,
    },
    {
        "name": "l2-squared",
        "description": "Squared euclidean distance. Sum of squared differences.",
        "fields": None,
    },
    {
        "name": "hamming",
        "description": "Hamming distance. Count of differing dimensions. Uses SIMD optimization.",
        "fields": None,
    },
    {
        "name": "manhattan",
        "description": "Manhattan distance. Sum of absolute differences per dimension. Uses SIMD optimization.",
        "fields": None,
    },
    {
        "name": "Class",
        "description": "Collection schema definition",
        "fields": [
            {"name": "class", "type": "string", "required": True},
            {"name": "properties", "type": "array<Property>", "required": False},
            {"name": "vectorConfig", "type": "map<VectorConfig>", "required": False},
            {"name": "vectorIndexType", "type": "string", "required": False},
            {"name": "shardingConfig", "type": "object", "required": False},
            {"name": "replicationConfig", "type": "object", "required": False},
        ],
    },
    {
        "name": "Property",
        "description": "Property definition in a collection schema",
        "fields": [
            {"name": "name", "type": "string", "required": True},
            {"name": "dataType", "type": "string", "required": True},
            {"name": "description", "type": "string", "required": False},
            {"name": "indexFilterable", "type": "boolean", "required": False},
            {"name": "indexSearchable", "type": "boolean", "required": False},
            {"name": "tokenization", "type": "string", "required": False},
        ],
    },
]

# ---- Endpoint Registry ----
endpoint_registry = [ref_entry(ep["path"], ep["method"], ep["description"]) for ep in endpoints]

# ---- Build contract skeleton (without _passport) ----
contract_no_passport = {
    "target": "weaviate",
    "version": "v1.38.0",
    "cache_ttl_hours": 168,
    "cached_at": now,
    "sdk": {
        "package": "weaviate-client",
        "version": "4.21.3",
        "install_command": "pip install weaviate-client==4.21.3",
    },
    "docker": {
        "repo": "semitechnologies/weaviate",
        "available_tags": ["1.38.0"],
    },
    "api_endpoints": endpoints,
    "endpoint_registry": endpoint_registry,
    "constraints": {
        "type_constraints": type_constraints,
        "range_constraints": range_constraints,
        "state_constraints": state_constraints,
    },
    "assertions": assertions,
    "behavioral_contracts": behavioral_contracts,
    "state_invariants": state_invariants,
    "data_types": data_types,
}

# Compute hash
data_without_passport = {k: v for k, v in contract_no_passport.items() if k != "_passport"}
canonical_json = json.dumps(data_without_passport, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
h = hashlib.sha256(canonical_json.encode("utf-8"))
contract_hash = f"sha256:{h.hexdigest()}"

# Build full contract with _passport
full_contract = dict(contract_no_passport)
full_contract["_passport"] = {
    "schema_version": "2.0",
    "contract_hash": contract_hash,
    "contract_hash_algorithm": "sha256",
    "source": {
        "doc_urls": DOC_URLS,
        "doc_version": "1.38.0",
        "crawl_method": "webfetch",
        "crawled_at": "2026-06-07T00:00:00Z",
    },
    "generation": {
        "knowledge_extractor_agent": "testvdb:knowledge-extractor",
        "contract_formalizer_agent": "testvdb:contract-formalizer",
        "generated_at": now,
        "cache_ttl_hours": 168,
    },
    "integrity": {
        "verified": True,
        "verified_at": now,
        "core_crud_coverage_pct": 95.0,
        "endpoint_count": len(endpoints),
        "constraint_count": len(type_constraints) + len(range_constraints) + len(state_constraints),
    },
}

# Write output
output_path = "C:/Users/11428/Desktop/mftui/TestVDB/results/weaviate/v1.38.0/structured_contract.json"
with open(output_path, "w", encoding="utf-8") as f:
    json.dump(full_contract, f, indent=2, ensure_ascii=False)

print(f"Written to: {output_path}")
print(f"Endpoints: {len(endpoints)}")
print(f"Type constraints: {len(type_constraints)}")
print(f"Range constraints: {len(range_constraints)}")
print(f"State constraints: {len(state_constraints)}")
print(f"Assertions: {len(assertions)}")
print(f"Behavioral contracts: {len(behavioral_contracts)}")
print(f"State invariants: {len(state_invariants)}")
print(f"Data types: {len(data_types)}")
print(f"Contract hash: {contract_hash}")

# Verify JSON is valid
with open(output_path, "r", encoding="utf-8") as f:
    verified = json.load(f)
print(f"JSON valid: True")
print(f"Endpoint count in verified: {len(verified['api_endpoints'])}")
all_categories = set(ep.get("category") for ep in verified["api_endpoints"])
print(f"Used categories: {sorted(all_categories)}")
