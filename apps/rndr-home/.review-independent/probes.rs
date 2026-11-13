
#[test]
fn independent_review_full_chooser_cycle_arrows_and_refresh() {
    let mut a = app(); a.update();
    a.world_mut().insert_resource(Viewport::new(480.,650.));
    desktop::navigate(a.world_mut(), Route::Settings(3)); settle(&mut a);
    activate(&mut a, "settings:choose");
    key(&mut a, NamedKey::Home, false);
    for i in 0..9 { assert_eq!(focus_action(&a),format!("settings:{i}")); key(&mut a,NamedKey::ArrowDown,false); }
    assert_eq!(focus_action(&a),"settings:0");
    key(&mut a,NamedKey::ArrowUp,false); assert_eq!(focus_action(&a),"settings:8");
    key(&mut a,NamedKey::Tab,false); assert_eq!(focus_action(&a),"menu:close");
    key(&mut a,NamedKey::Tab,false); assert_eq!(focus_action(&a),"settings:0");
    key(&mut a,NamedKey::Tab,true); assert_eq!(focus_action(&a),"menu:close");
    key(&mut a,NamedKey::Tab,true); assert_eq!(focus_action(&a),"settings:8");
    key(&mut a,NamedKey::Escape,false); assert_eq!(focus_action(&a),"settings:choose");
    a.world_mut().resource_mut::<Desktop>().unwrap().reads.insert(Resource::Activity,ReadState::Ready(rows()));
    desktop::navigate(a.world_mut(), Route::Data(Resource::Activity)); settle(&mut a);
    activate(&mut a,"record:7");
    let serial=a.world_mut().resource_mut::<Desktop>().unwrap().begin(Resource::Activity).unwrap();
    settle(&mut a);
    assert_eq!(a.world().resource::<Desktop>().unwrap().selected,None);
    send(a.world_mut(),admin_backend::ReadReply {resource:Resource::Activity,serial,result:Ok(vec![Record::new("replacement",vec![])])});
    settle(&mut a);
    assert_eq!(a.world().resource::<Desktop>().unwrap().selected,None);
    assert!(a.world().entities_with::<Button>().into_iter().all(|e|a.world().get::<Button>(e).unwrap().action!="admin:back"));
}
