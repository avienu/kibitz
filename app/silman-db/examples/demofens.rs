fn main() {
    let conn = silman_db::db::open(std::path::Path::new("testdata/corpus/scid.sqlite")).unwrap();
    let (start, tokens) = silman_db::edit::game_tokens(&conn, 3727).unwrap();
    let mut board = start.clone();
    let mut ply = 0;
    for p in silman_db::movebin::mainline_of(&tokens) {
        if let silman_db::movebin::Ply::Move(m) = p {
            board.play(m);
            ply += 1;
            if [12, 30, 50].contains(&ply) {
                println!("ply {ply}: {board}");
            }
        }
    }
    println!("final ply {ply}: {board}");
}
