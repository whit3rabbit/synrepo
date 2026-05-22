use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{
    config::Config,
    core::ids::NodeId,
    store::sqlite::SqliteGraphStore,
    structure::graph::{EdgeKind, SymbolKind},
};
use std::fs;
use tempfile::{tempdir, TempDir};

fn compile_fixture(path: &str, source: &str) -> (TempDir, SqliteGraphStore) {
    let repo = tempdir().unwrap();
    let full_path = repo.path().join(path);
    fs::create_dir_all(full_path.parent().unwrap()).unwrap();
    fs::write(&full_path, source).unwrap();
    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    (repo, graph)
}

fn assert_route_refs(graph: &SqliteGraphStore, path: &str, route_name: &str, target_qname: &str) {
    let file = graph.file_by_path(path).unwrap().unwrap();
    let route = graph
        .symbols_for_file(file.id)
        .unwrap()
        .into_iter()
        .find(|symbol| symbol.kind == SymbolKind::Route && symbol.display_name == route_name)
        .unwrap_or_else(|| panic!("route symbol {route_name} must exist"));
    let refs = graph
        .outbound(NodeId::Symbol(route.id), Some(EdgeKind::References))
        .unwrap();
    assert!(
        refs.iter().any(|edge| {
            let NodeId::Symbol(symbol_id) = edge.to else {
                return false;
            };
            graph
                .get_symbol(symbol_id)
                .unwrap()
                .is_some_and(|symbol| symbol.qualified_name == target_qname)
        }),
        "expected route {route_name} to reference {target_qname}; got: {refs:?}"
    );
}

#[test]
fn rust_axum_route_references_handler_symbol() {
    let (_repo, graph) = compile_fixture(
        "src/lib.rs",
        "use axum::{routing::get, Router};\nfn app() { Router::new().route(\"/users\", get(list_users)); }\nfn list_users() {}\n",
    );
    assert_route_refs(&graph, "src/lib.rs", "GET /users", "list_users");
}

#[test]
fn typescript_express_route_references_handler_symbol() {
    let (_repo, graph) = compile_fixture(
        "src/server.ts",
        "const router = express.Router();\nrouter.post(\"/users\", createUser);\nfunction createUser() {}\n",
    );
    assert_route_refs(&graph, "src/server.ts", "POST /users", "createUser");
}

#[test]
fn python_fastapi_and_django_routes_reference_handlers() {
    let (_repo, graph) = compile_fixture(
        "app.py",
        "from django.urls import path\n@router.get('/users')\ndef list_users(): pass\nclass AccountView:\n    def get(self): pass\nurlpatterns = [path('accounts/', AccountView.as_view())]\n",
    );
    assert_route_refs(&graph, "app.py", "GET /users", "list_users");
    assert_route_refs(&graph, "app.py", "ANY /accounts", "AccountView");
}

#[test]
fn typescript_express_middleware_and_nest_routes_reference_handlers() {
    let (_repo, graph) = compile_fixture(
        "src/server.ts",
        "router.post('/users', auth, createUser);\nfunction createUser() {}\n@Controller('/users')\nclass UsersController {\n  @Get(':id')\n  show() {}\n}\n",
    );
    assert_route_refs(&graph, "src/server.ts", "POST /users", "createUser");
    assert_route_refs(
        &graph,
        "src/server.ts",
        "GET /users/:id",
        "UsersController::show",
    );
}

#[test]
fn spring_route_references_controller_method() {
    let (_repo, graph) = compile_fixture(
        "src/main/java/com/acme/UsersController.java",
        "import org.springframework.web.bind.annotation.*;\n@RequestMapping(\"/api\")\nclass UsersController {\n  @GetMapping(\"/users\")\n  public String listUsers() { return \"\"; }\n}\n",
    );
    assert_route_refs(
        &graph,
        "src/main/java/com/acme/UsersController.java",
        "GET /api/users",
        "UsersController::listUsers",
    );
}

#[test]
fn go_gin_route_references_handler() {
    let (_repo, graph) = compile_fixture(
        "main.go",
        "package main\nfunc routes() { r.GET(\"/users\", listUsers) }\nfunc listUsers() {}\n",
    );
    assert_route_refs(&graph, "main.go", "GET /users", "listUsers");
}

#[test]
fn csharp_aspnet_route_references_action() {
    let (_repo, graph) = compile_fixture(
        "Controllers/UsersController.cs",
        "[Route(\"api/users\")]\npublic class UsersController {\n  [HttpGet(\"{id}\")]\n  public IActionResult Show() { return Ok(); }\n}\n",
    );
    assert_route_refs(
        &graph,
        "Controllers/UsersController.cs",
        "GET /api/users/{id}",
        "UsersController::Show",
    );
}

#[test]
fn php_laravel_route_and_resource_reference_actions() {
    let (_repo, graph) = compile_fixture(
        "routes/web.php",
        "<?php\nRoute::get('/users', [UserController::class, 'index']);\nRoute::resource('posts', PostController::class);\nclass UserController { function index() {} }\nclass PostController { function index() {} }\n",
    );
    assert_route_refs(
        &graph,
        "routes/web.php",
        "GET /users",
        "UserController::index",
    );
    assert_route_refs(
        &graph,
        "routes/web.php",
        "GET /posts",
        "PostController::index",
    );
}

#[test]
fn ruby_rails_route_and_resource_reference_actions() {
    let (_repo, graph) = compile_fixture(
        "config/routes.rb",
        "Rails.application.routes.draw do\n  get '/users', to: 'users#index'\n  resources :posts\nend\nclass UsersController\n  def index; end\nend\nclass PostsController\n  def index; end\nend\n",
    );
    assert_route_refs(
        &graph,
        "config/routes.rb",
        "GET /users",
        "UsersController::index",
    );
    assert_route_refs(
        &graph,
        "config/routes.rb",
        "GET /posts",
        "PostsController::index",
    );
}

#[test]
fn unresolved_include_emits_route_without_reference() {
    let (_repo, graph) = compile_fixture(
        "urls.py",
        "from django.urls import include, path\nurlpatterns = [path('api/', include('api.urls'))]\n",
    );
    let file = graph.file_by_path("urls.py").unwrap().unwrap();
    let route = graph
        .symbols_for_file(file.id)
        .unwrap()
        .into_iter()
        .find(|symbol| symbol.kind == SymbolKind::Route && symbol.display_name == "ANY /api")
        .expect("include route symbol must exist");
    let refs = graph
        .outbound(NodeId::Symbol(route.id), Some(EdgeKind::References))
        .unwrap();
    assert!(refs.is_empty(), "include route should not guess a handler");
}
